use std::{error::Error, sync::Arc};

use libp2p::{
  futures::{lock, StreamExt},
  mdns, noise,
  swarm::{NetworkBehaviour, SwarmEvent},
  tcp, yamux, Multiaddr, PeerId, Swarm, SwarmBuilder,
};
use log::{debug, error, info};
use multiconnect_protocol::{
  daemon::peer::PeerFound, p2p::Peer, Packet
};
use tokio::{select, sync::Mutex};
use tracing_subscriber::EnvFilter;

use crate::SharedDaemon;

#[derive(NetworkBehaviour)]
struct MulticonnectBehavior {
  mnds: mdns::tokio::Behaviour,
}

impl MulticonnectBehavior {
  pub fn new(key: &libp2p::identity::Keypair) -> Result<Self, Box<dyn Error>> {
    let mnds_cfg = mdns::Config { query_interval: std::time::Duration::from_secs(1), ..Default::default() };

    Ok(Self { mnds: mdns::tokio::Behaviour::new(mnds_cfg, key.public().to_peer_id())? })
  }
}

pub struct NetworkManager {}

impl NetworkManager {
  pub async fn new(daemon: SharedDaemon) -> Result<Arc<Mutex<Self>>, Box<dyn Error>> {
    let _ = tracing_subscriber::fmt().with_env_filter(EnvFilter::from_default_env()).try_init();
    let (p_sender, mut p_receiver) =
      tokio::sync::mpsc::channel(100);

    debug!("Initializing new swarm");

    let mut swarm = SwarmBuilder::with_new_identity()
        .with_tokio()
        .with_tcp(tcp::Config::default(), noise::Config::new, yamux::Config::default)?
        .with_quic()
        .with_behaviour(|key| MulticonnectBehavior::new(key).unwrap())?
        .build();

    let mut bound = false;
    for port in 1590..=1600 {
      let addr: Multiaddr = format!("/ip4/0.0.0.0/tcp/{}", port).parse()?;
      if swarm.listen_on(addr.clone()).is_ok() {
        info!("Listening on {:?}", addr);
        bound = true;
        break;
      }
    }

    if !bound {
      error!("Failed to bind to any port in the range 1590-1600");
      return Err("Failed to bind to any port in the range 1590-1600".into());
    }


    let p2p_task = tokio::spawn(async move {
      loop {
        info!("Looping");
        tokio::select! {
          event = swarm.select_next_some() => match event {
            SwarmEvent::NewListenAddr { address, ..} => {
              info!("Multiconnect listing on {:?}", address);
            }
            SwarmEvent::Behaviour(MulticonnectBehaviorEvent::Mnds(mdns::Event::Discovered(discoverd))) => {
              for (peer_id, multiaddr) in discoverd {
                info!("Discoverd peer: id = {}, multiaddr = {}", peer_id, multiaddr);
                let peer = Peer { peer_id, multiaddr };
                info!("Sending peer");
                let _ = p_sender.send(peer).await;
              }
            }
            _ => {
              info!("Event: {:?}", event);
            }
          }
        }
      }
    });

    let daemon_task = tokio::spawn(async move {
      while let Some(peer) = p_receiver.recv().await {
        let mut locked = daemon.lock().await;
        info!("Locked daemon");
        locked.add_to_queue(Packet::PeerFound(PeerFound::new(peer))).await;
    }
    });

    Ok(Arc::new(Mutex::new(Self {})))
  }
}

#[cfg(test)]
mod tests {
  use super::*;
}
