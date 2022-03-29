use std::{net::IpAddr, path::PathBuf, time::Duration};

use anyhow::anyhow;
use notify::{watcher, DebouncedEvent, Watcher};
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;
use tracing::error;
use trust_dns_resolver::{config::Protocol, Name};

use crate::resolver::Resolver;

#[derive(Debug, Clone)]
pub struct ConfigUpdate {
    pub config_filename: PathBuf,
    pub config: Option<Config>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    domain_name: Name,
    forwarders: Vec<IpAddr>,
    protocol: Protocol,
}

impl Config {
    pub fn new(filename: PathBuf) -> Result<Self, anyhow::Error> {
        match serde_yaml::from_str(&std::fs::read_to_string(filename)?) {
            Ok(config) => Ok(config),
            Err(e) => Err(anyhow!("Error parsing YAML configuration: {}", e)),
        }
    }

    pub fn make_resolvers(&self) -> Result<Resolver, anyhow::Error> {
        Err(anyhow!("unimplemented"))
    }
}

#[derive(Debug, Clone)]
pub struct ConfigDir(PathBuf);

impl ConfigDir {
    pub fn new(path: PathBuf) -> Self {
        Self(path)
    }

    pub async fn watcher(
        self,
        configs: mpsc::Sender<ConfigUpdate>,
        closer: std::sync::mpsc::Receiver<()>,
    ) {
        let (tx, mut rx) = mpsc::unbounded_channel();

        let reader_tx = tx.clone();
        let reader_self = self.clone();
        std::thread::spawn(move || {
            for item in std::fs::read_dir(reader_self.0.clone()).expect("Cannot read directory") {
                let item = item.expect("cannot stat file");
                if let Ok(meta) = item.metadata() {
                    if meta.is_file() {
                        reader_tx
                            .send(Ok(DebouncedEvent::NoticeWrite(item.path())))
                            .expect("cannot send item");
                    }
                }
            }
        });

        let watcher_self = self.clone();

        std::thread::spawn(move || {
            let (s, r) = std::sync::mpsc::channel();
            let mut watcher =
                watcher(s, Duration::new(1, 0)).expect("Could not initialize watcher");

            watcher
                .watch(watcher_self.0.clone(), notify::RecursiveMode::NonRecursive)
                .expect(&format!(
                    "Could not watch directory: {}",
                    watcher_self.0.display()
                ));

            loop {
                let item = r.recv();
                tx.send(item).expect("cannot send over channel");

                if let Ok(()) = closer.try_recv() {
                    return;
                }
            }
        });

        loop {
            let item = rx.recv().await.expect("receive failure from watcher");
            match item {
                Ok(DebouncedEvent::NoticeWrite(item)) | Ok(DebouncedEvent::Create(item)) => {
                    if let Ok(meta) = item.clone().metadata() {
                        if !meta.is_dir() {
                            let config = Config::new(item.clone());
                            match config {
                                Ok(config) => {
                                    configs
                                        .send(ConfigUpdate {
                                            config_filename: item,
                                            config: Some(config),
                                        })
                                        .await
                                        .expect(&format!(
                                            "could not deliver configuration {} from notify",
                                            self.0.display(),
                                        ));
                                }
                                Err(e) => error!("{:#?}: {}", item.file_name(), e),
                            }
                        }
                    }
                }
                Ok(DebouncedEvent::NoticeRemove(item)) => configs
                    .send(ConfigUpdate {
                        config_filename: item,
                        config: None,
                    })
                    .await
                    .expect("could not delete configuration"),
                Err(e) => error!("Error watching files: {}", e),
                Ok(_) => {}
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Config, ConfigDir};

    #[test]
    fn test_config_constructor() {
        let mut count = 0;

        for file in std::fs::read_dir("testdata/configs/valid").unwrap() {
            let file = file.unwrap();
            if file.metadata().unwrap().is_file() {
                let res = Config::new(file.path());
                assert!(res.is_ok());
                count += 1;
            }
        }

        assert!(count > 0);
        count = 0;

        for file in std::fs::read_dir("testdata/configs/invalid").unwrap() {
            let file = file.unwrap();
            if file.metadata().unwrap().is_file() {
                let res = Config::new(file.path());
                assert!(res.is_err());
                count += 1;
            }
        }

        assert!(count > 0);
    }

    const TEMPORARY_CONFIG: &str = "temporary.yaml";

    #[tokio::test(flavor = "multi_thread")]
    async fn test_config_scanner() {
        use tempdir::TempDir;

        let dir = TempDir::new("polyresolver_config").unwrap();
        let dirpath = dir.into_path();

        std::fs::copy("testdata/configs/valid/one.yaml", dirpath.join("one.yaml")).unwrap();

        let dirscanner = ConfigDir::new(dirpath.clone());
        let (s, mut r) = tokio::sync::mpsc::channel(1);
        let (closer_s, closer_r) = std::sync::mpsc::channel();

        tokio::spawn(dirscanner.watcher(s, closer_r));
        let (mut filecount, mut recvcount) = (0, 0);

        for file in std::fs::read_dir(dirpath.clone()).unwrap() {
            if file.unwrap().metadata().unwrap().is_file() {
                filecount += 1
            }
        }

        for _ in r.recv().await {
            recvcount += 1;
        }

        assert!(recvcount == filecount);
        recvcount = 0;

        // create a new config
        std::fs::write(
            dirpath.join(TEMPORARY_CONFIG),
            r#"domain_name: foo
forwarders:
    - 127.0.0.1
    - 192.168.1.1
protocol: udp
"#,
        )
        .unwrap();

        for _ in r.recv().await {
            recvcount += 1;
        }

        assert!(recvcount == 1);

        closer_s.send(()).unwrap();
    }
}
