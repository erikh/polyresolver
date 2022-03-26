use std::{
    net::{IpAddr, SocketAddr},
    str::FromStr,
    time::Duration,
};

use openssl::{
    pkey::{PKey, Private},
    stack::Stack,
    x509::X509,
};
use tokio::net::{TcpListener, UdpSocket};
use tracing::{error, info};
use trust_dns_server::ServerFuture;

use crate::catalog::init_catalog;

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
