use std::{
    net::IpAddr,
    path::PathBuf,
    time::{Duration, SystemTime},
};

use anyhow::anyhow;
use serde::{Deserialize, Serialize};
use tokio::{sync::mpsc, time::interval};
use tracing::error;
use trust_dns_resolver::{config::Protocol, Name};

use crate::resolver::Resolver;

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

    pub async fn scan(self, configs: mpsc::Sender<Config>) {
        let mut interval = interval(Duration::new(1, 0));

        // FIXME only scan new files. This probably races and we'll need inotify etc later. this is
        //       just simpler right now.
        let mut now = std::time::UNIX_EPOCH;

        loop {
            match std::fs::read_dir(self.0.clone()) {
                Ok(dir) => {
                    for item in dir {
                        if let Ok(item) = item {
                            if let Ok(meta) = item.metadata() {
                                if !meta.is_dir()
                                    && (meta.modified().is_ok() && meta.modified().unwrap() > now)
                                {
                                    let config = Config::new(item.path());
                                    match config {
                                        Ok(config) => match configs.send(config).await {
                                            Ok(_) => {}
                                            Err(e) => error!("{:#?}: {}", item.file_name(), e),
                                        },
                                        Err(e) => error!("{:#?}: {}", item.file_name(), e),
                                    }
                                }
                            }
                        }
                    }
                }
                Err(e) => error!("Could not read configuration directory: {}", e),
            }

            now = SystemTime::now();
            interval.tick().await;
        }
    }
}
