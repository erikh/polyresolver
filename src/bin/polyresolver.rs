use std::{net::IpAddr, path::PathBuf, str::FromStr, time::Duration};

use anyhow::anyhow;
use tracing_log::log_tracer;
use tracing_subscriber::FmtSubscriber;

use polyresolver::listener::listen;

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    let mut args = std::env::args().skip(1);
    let config_dir = if let Some(arg) = args.next() {
        PathBuf::from_str(&arg)?
    } else {
        return Err(anyhow!(
            "You must provide us with a configuration directory"
        ));
    };

    let ip = if let Some(arg) = args.next() {
        Some(IpAddr::from_str(&arg)?)
    } else {
        None
    };

    log_tracer::Builder::new().init()?;

    let subscriber = FmtSubscriber::builder()
        .with_max_level(tracing::Level::TRACE)
        .finish();

    tracing::subscriber::set_global_default(subscriber).expect("setting default subscriber failed");

    listen(ip, config_dir, Duration::new(1, 0), None, None).await
}
