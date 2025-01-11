use std::{error::Error, sync::Arc};

use libp2p::{futures::StreamExt, mdns, noise, swarm::{NetworkBehaviour, SwarmEvent}, tcp, yamux, Swarm, SwarmBuilder};
use log::{debug, info};
use tokio::sync::Mutex;
use tracing_subscriber::EnvFilter;

#[derive(NetworkBehaviour)]
struct Behavior {
  mnds: mdns::tokio::Behaviour
}

pub struct NetworkManager {
  swarm: Arc<Mutex<Swarm<Behavior>>>
}

impl NetworkManager {
  pub async fn new() -> Result<Self, Box<dyn Error>> {
    let _ = tracing_subscriber::fmt().with_env_filter(EnvFilter::from_default_env()).try_init();

    debug!("Initializing new swarm");
    let mut swarm = Arc::new(Mutex::new(SwarmBuilder::with_new_identity()
      .with_tokio()
      .with_tcp(
        tcp::Config::default(),
        noise::Config::new,
        yamux::Config::default
      )?
      .with_behaviour(|key| {
        Ok(Behavior {
          mnds: mdns::tokio::Behaviour::new(
            mdns::Config::default(), 
            key.public().to_peer_id()
          )?,
        })
      })?
      .build()));

    
    let clone = swarm.clone();
    clone.lock().await.listen_on("/ip4/0.0.0.0/tcp/0".parse()?)?;
    tokio::spawn(async move {
      loop {
        match clone.lock().await.select_next_some().await {
          SwarmEvent::NewListenAddr { address, ..} => {
            info!("Listening in {:?}", address);
          }
          _ => {}
        }
      }
    });

    Ok(Self { swarm })
  }
}

#[cfg(test)]
mod tests {
  use super::*;
}
