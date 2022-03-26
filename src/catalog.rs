use std::{net::IpAddr, sync::Arc};

use trust_dns_resolver::Name;
use trust_dns_server::authority::Catalog;

use crate::{forwarder::Forwarder, resolver::create_resolver};

pub async fn init_catalog(
    nameservers: Vec<IpAddr>,
    search_names: Vec<Name>,
) -> Result<Catalog, anyhow::Error> {
    let mut catalog = Catalog::default();

    let forwarder = Forwarder {
        resolver: create_resolver(nameservers, search_names)?,
        domain_name: Name::root().into(),
    };

    catalog.upsert(Name::root().into(), Box::new(Arc::new(forwarder)));

    Ok(catalog)
}
