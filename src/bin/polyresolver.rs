use std::{
    net::{IpAddr, SocketAddr},
    str::FromStr,
    sync::Arc,
    time::Duration,
};

use async_trait::async_trait;
use openssl::{
    pkey::{PKey, Private},
    stack::Stack,
    x509::X509,
};
use tokio::net::{TcpListener, UdpSocket};
use tracing::{error, info};
use tracing_log::log_tracer;
use tracing_subscriber::FmtSubscriber;
use trust_dns_resolver::{
    config::{NameServerConfig, ResolverConfig, ResolverOpts},
    error::ResolveError,
    lookup,
    name_server::{GenericConnection, GenericConnectionProvider, TokioRuntime},
    proto::{
        rr::{Record, RecordType},
        xfer::DnsRequestOptions,
    },
    AsyncResolver, Name,
};
use trust_dns_server::{
    authority::{Authority, Catalog, LookupError, LookupObject},
    client::rr::LowerName,
    ServerFuture,
};

async fn init_catalog(
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

#[derive(Clone)]
pub struct Forwarder {
    resolver: Resolver,
    domain_name: LowerName,
}

#[async_trait]
impl Authority for Forwarder {
    type Lookup = ForwardLookup;

    fn zone_type(&self) -> trust_dns_server::authority::ZoneType {
        trust_dns_server::authority::ZoneType::Forward
    }

    fn is_axfr_allowed(&self) -> bool {
        false
    }

    async fn update(
        &self,
        _update: &trust_dns_server::authority::MessageRequest,
    ) -> trust_dns_server::authority::UpdateResult<bool> {
        Ok(false)
    }

    fn origin(&self) -> &trust_dns_server::client::rr::LowerName {
        &self.domain_name
    }

    async fn lookup(
        &self,
        name: &trust_dns_server::client::rr::LowerName,
        rtype: RecordType,
        _lookup_options: trust_dns_server::authority::LookupOptions,
    ) -> Result<Self::Lookup, LookupError> {
        let lookup = self
            .resolver
            .lookup(name, rtype, DnsRequestOptions::default())
            .await;

        lookup.map(ForwardLookup).map_err(LookupError::from)
    }

    async fn search(
        &self,
        request_info: trust_dns_server::server::RequestInfo<'_>,
        lookup_options: trust_dns_server::authority::LookupOptions,
    ) -> Result<Self::Lookup, LookupError> {
        self.lookup(
            request_info.query.name(),
            request_info.query.query_type(),
            lookup_options,
        )
        .await
    }

    async fn get_nsec_records(
        &self,
        _name: &trust_dns_server::client::rr::LowerName,
        _lookup_options: trust_dns_server::authority::LookupOptions,
    ) -> Result<Self::Lookup, LookupError> {
        Err(LookupError::from(std::io::Error::new(
            std::io::ErrorKind::Other,
            "this kind of lookup is not supported",
        )))
    }
}

pub struct ForwardLookup(lookup::Lookup);

impl LookupObject for ForwardLookup {
    fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    fn iter<'a>(&'a self) -> Box<dyn Iterator<Item = &'a Record> + Send + 'a> {
        Box::new(self.0.record_iter())
    }

    fn take_additionals(&mut self) -> Option<Box<dyn LookupObject>> {
        None
    }
}

type Resolver = AsyncResolver<GenericConnection, GenericConnectionProvider<TokioRuntime>>;

fn create_resolver(
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
        });
    }

    trust_dns_resolver::TokioAsyncResolver::tokio(resolver_config, opts)
}

// listener routine for TCP, UDP, and TLS.
pub async fn listen(
    listen_ip: Option<IpAddr>,
    nameservers: Vec<IpAddr>,
    tcp_timeout: Duration,
    certs: Option<(X509, Option<Stack<X509>>)>,
    key: Option<PKey<Private>>,
) -> Result<(), anyhow::Error> {
    let listen_ip = listen_ip.unwrap_or(IpAddr::from_str("127.0.0.1")?);

    let sa = SocketAddr::new(listen_ip, 53);
    let tcp = TcpListener::bind(sa).await?;
    let udp = UdpSocket::bind(sa).await?;

    let mut sf = ServerFuture::new(init_catalog(nameservers, Vec::new()).await?);

    if certs.is_some() && key.is_some() {
        info!("Configuring DoT Listener");

        let tls = TcpListener::bind(SocketAddr::new(listen_ip, 853)).await?;
        match sf.register_tls_listener(tls, tcp_timeout, (certs.unwrap(), key.clone().unwrap())) {
            Ok(_) => {}
            Err(e) => error!("Cannot start DoT listener: {}", e),
        }
    }

    sf.register_socket(udp);
    sf.register_listener(tcp, tcp_timeout);

    match sf.block_until_done().await {
        Ok(_) => Ok(()),
        Err(e) => Err(anyhow::anyhow!("{}", e)),
    }
}

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    let mut args = std::env::args();
    let ip = if let Some(arg) = args.nth(1) {
        Some(IpAddr::from_str(&arg)?)
    } else {
        None
    };

    let nameservers = args
        .map(|f| IpAddr::from_str(&f).expect("invalid IP for nameserver"))
        .collect();

    log_tracer::Builder::new().init()?;

    // a builder for `FmtSubscriber`.
    let subscriber = FmtSubscriber::builder()
        // all spans/events with a level higher than TRACE (e.g, debug, info, warn, etc.)
        // will be written to stdout.
        .with_max_level(tracing::Level::INFO)
        // completes the builder.
        .finish();

    tracing::subscriber::set_global_default(subscriber).expect("setting default subscriber failed");

    listen(ip, nameservers, Duration::new(1, 0), None, None).await
}
