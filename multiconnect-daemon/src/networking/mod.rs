pub mod peer;

use std::{error::Error, sync::Arc};

use libp2p::{
  futures::{lock, StreamExt},
  mdns, noise,
  swarm::{NetworkBehaviour, SwarmEvent},
  tcp, yamux, Multiaddr, Swarm, SwarmBuilder,
};
use log::{debug, error, info};
use peer::Peer;
use tokio::{select, sync::Mutex};
use tracing_subscriber::EnvFilter;

#[derive(NetworkBehaviour)]
struct MulticonnectBehavior {
  mnds: mdns::tokio::Behaviour,
}

impl MulticonnectBehavior {
  pub fn new(key: &libp2p::identity::Keypair) -> Result<Self, Box<dyn Error>> {
    let mnds_cfg = mdns::Config { query_interval: std::time::Duration::from_secs(2), ..Default::default() };

    Ok(Self { mnds: mdns::tokio::Behaviour::new(mnds_cfg, key.public().to_peer_id())? })
  }
}

pub struct NetworkManager {
  swarm: Arc<Mutex<Swarm<MulticonnectBehavior>>>,
  peers: Arc<Mutex<Vec<Peer>>>,
}

impl NetworkManager {
  pub async fn new() -> Result<Arc<Mutex<Self>>, Box<dyn Error>> {
    let _ = tracing_subscriber::fmt().with_env_filter(EnvFilter::from_default_env()).try_init();

    let peers = Arc::new(Mutex::new(Vec::new()));
    debug!("Initializing new swarm");

    let swarm = Arc::new(Mutex::new(
      SwarmBuilder::with_new_identity()
        .with_tokio()
        .with_tcp(tcp::Config::default(), noise::Config::new, yamux::Config::default)?
        .with_quic()
        .with_behaviour(|key| MulticonnectBehavior::new(key).unwrap())?
        .build(),
    ));

    let mut bound = false;
    for port in 1590..=1600 {
      let addr: Multiaddr = format!("/ip4/0.0.0.0/tcp/{}", port).parse()?;
      let mut locked = swarm.lock().await;
      if locked.listen_on(addr.clone()).is_ok() {
        info!("Listening on {:?}", addr);
        bound = true;
        break;
      }
    }

    if !bound {
      error!("Failed to bind to any port in the range 1590-1600");
      return Err("Failed to bind to any port in the range 1590-1600".into());
    }

    let swarm_clone = swarm.clone();
    let peers_clone = peers.clone();

    tokio::spawn(async move {
      let mut locked = swarm_clone.lock().await;
      loop {
        select! {
          event = locked.select_next_some() => match event {
            SwarmEvent::NewListenAddr { address, ..} => {
              info!("Multiconnect listing on {:?}", address);
            }
            SwarmEvent::Behaviour(MulticonnectBehaviorEvent::Mnds(mdns::Event::Discovered(discoverd))) => {
              for (peer_id, multiaddr) in discoverd {
                info!("Discoverd peer: id = {}, multiaddr = {}", peer_id, multiaddr);
                let mut locked = peers_clone.lock().await;
                locked.push(Peer { peer_id, multiaddr });
              }
            }
            _ => {}
          }
        }
      }
    });

    Ok(Arc::new(Mutex::new(Self { swarm, peers })))
  }

  pub async fn list_peers(&self) -> Vec<Peer> {
    let locked = self.peers.lock().await;
    locked.clone()
  }
}

#[cfg(test)]
mod tests {
  use super::*;
}
