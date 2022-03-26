use std::{net::IpAddr, str::FromStr, time::Duration};

use tracing_log::log_tracer;
use tracing_subscriber::FmtSubscriber;

use polyresolver::listener::listen;

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

    let subscriber = FmtSubscriber::builder()
        .with_max_level(tracing::Level::INFO)
        .finish();

    tracing::subscriber::set_global_default(subscriber).expect("setting default subscriber failed");

    listen(ip, nameservers, Duration::new(1, 0), None, None).await
}
