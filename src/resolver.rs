use std::{
    collections::BTreeMap,
    net::{IpAddr, SocketAddr},
    path::PathBuf,
    time::Duration,
};

use anyhow::anyhow;
use trust_dns_resolver::{
    config::{NameServerConfig, ResolverConfig, ResolverOpts},
    error::ResolveError,
    name_server::{GenericConnection, GenericConnectionProvider, TokioRuntime},
    AsyncResolver, Name,
};

use crate::{
    catalog::LockedCatalog,
    config::{Config, ConfigUpdate},
};

pub type Resolver = AsyncResolver<GenericConnection, GenericConnectionProvider<TokioRuntime>>;

pub fn create_resolver(
    nameservers: Vec<IpAddr>,
    search_names: Vec<Name>,
) -> Result<Resolver, ResolveError> {
    let mut opts = ResolverOpts::default();
    opts.timeout = Duration::new(1, 0);
    opts.cache_size = 0;
    opts.rotate = true;
    opts.use_hosts_file = false;
    opts.positive_min_ttl = Some(Duration::new(0, 0));
    opts.positive_max_ttl = Some(Duration::new(0, 0));
    opts.negative_min_ttl = Some(Duration::new(0, 0));
    opts.negative_max_ttl = Some(Duration::new(0, 0));

    let mut resolver_config = ResolverConfig::new();
    for nameserver in nameservers {
        for name in search_names.clone() {
            resolver_config.add_search(name);
        }

        resolver_config.add_name_server(NameServerConfig {
            bind_addr: None,
            socket_addr: SocketAddr::new(nameserver, 53),
            protocol: trust_dns_resolver::config::Protocol::Udp,
            tls_dns_name: None,
            trust_nx_responses: true,
            tls_config: None,
        });
    }

    trust_dns_resolver::TokioAsyncResolver::tokio(resolver_config, opts)
}

#[derive(Clone)]
pub struct ResolverCollection {
    configs: BTreeMap<PathBuf, Config>,
    catalog: LockedCatalog,
}

impl ResolverCollection {
    pub fn new(catalog: LockedCatalog) -> Self {
        Self {
            catalog,
            configs: BTreeMap::new(),
        }
    }

    pub async fn set_config(&mut self, update: ConfigUpdate) -> Result<(), anyhow::Error> {
        if update.config.is_none() {
            return Err(anyhow!(
                "Tried to insert deleted configuration {}",
                update.config_filename.display()
            ));
        }

        let config = update.config.unwrap();

        self.configs.insert(update.config_filename, config.clone());

        self.catalog
            .write()
            .await
            .upsert(config.domain_name().into(), config.forwarder()?);

        Ok(())
    }

    pub async fn remove_config(&mut self, update: ConfigUpdate) -> Result<(), anyhow::Error> {
        let config = self.configs.remove(&update.config_filename);

        if let Some(config) = config {
            self.catalog
                .write()
                .await
                .remove(&config.domain_name().into());
        }

        Ok(())
    }
}
