use std::{path::PathBuf, sync::Arc};

use async_trait::async_trait;
use tokio::sync::{mpsc, RwLock};
use trust_dns_server::{
    authority::Catalog,
    server::{Request, RequestHandler, ResponseHandler, ResponseInfo},
};

use crate::{config::ConfigWatcher, resolver::ResolverCollection};

pub type LockedCatalog = Arc<RwLock<Catalog>>;

#[derive(Clone)]
pub struct PolyCatalog {
    catalog: LockedCatalog,
    resolvers: ResolverCollection,
}

impl PolyCatalog {
    pub fn new() -> Self {
        let catalog = LockedCatalog::default();
        Self {
            resolvers: ResolverCollection::new(catalog.clone()),
            catalog,
        }
    }

    pub async fn sync_catalog(
        mut self,
        config_dir: PathBuf,
        closer_r: std::sync::mpsc::Receiver<()>,
    ) {
        let watcher = ConfigWatcher::new(config_dir);
        let (tx, mut rx) = mpsc::channel(1);
        let (watcher_closer_s, watcher_closer_r) = std::sync::mpsc::channel();

        tokio::spawn(watcher.watch(tx, watcher_closer_r));

        loop {
            let update = rx.recv().await.expect("Unable to receive from channel");
            match update.config {
                Some(_) => self
                    .resolvers
                    .set_config(update)
                    .await
                    .expect("Unable to set resolver configuration"),
                None => self
                    .resolvers
                    .remove_config(update)
                    .await
                    .expect("Unable to remove resolver configuration"),
            }

            if closer_r.try_recv().is_ok() {
                watcher_closer_s.send(()).expect("couldn't close watcher");
                return;
            }
        }
    }
}

#[async_trait]
impl RequestHandler for PolyCatalog {
    async fn handle_request<R: ResponseHandler>(
        &self,
        request: &Request,
        response_handle: R,
    ) -> ResponseInfo {
        self.catalog
            .read()
            .await
            .handle_request(request, response_handle)
            .await
    }
}
