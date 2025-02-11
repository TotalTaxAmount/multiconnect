mod pairing;
mod store;

use std::{error::Error, hash::Hash, rc::Rc, sync::Arc, time::Duration};

use libp2p::{
  futures::{lock, StreamExt},
  identity::{self, Keypair},
  mdns, noise,
  request_response::{self, Config, Event, ProtocolSupport},
  swarm::{NetworkBehaviour, SwarmEvent},
  tcp, yamux, Multiaddr, PeerId, Swarm, SwarmBuilder,
};
use log::{debug, error, info, trace};
use multiconnect_protocol::{
  peer::{PeerExpired, PeerFound, PeerPairRequest},
  Packet, Peer,
};
use pairing::PairingCodec;
use store::Store;
use tokio::{select, sync::Mutex};
use tracing_subscriber::EnvFilter;

use crate::SharedDaemon;

#[derive(NetworkBehaviour)]
struct MulticonnectBehavior {
  mnds: mdns::tokio::Behaviour,
  pairing: request_response::Behaviour<PairingCodec>,
}

impl MulticonnectBehavior {
  pub fn new(key: &libp2p::identity::Keypair) -> Result<Self, Box<dyn Error>> {
    let mnds_cfg = mdns::Config {
      ttl: Duration::from_secs(5),
      query_interval: std::time::Duration::from_secs(1),
      ..Default::default()
    };

    let pairing_protocol = request_response::Behaviour::<PairingCodec>::new(
      vec![("/pairing/1".into(), ProtocolSupport::Full)],
      Config::default(),
    );

    Ok(Self { mnds: mdns::tokio::Behaviour::new(mnds_cfg, key.public().to_peer_id())?, pairing: pairing_protocol })
  }
}

pub struct NetworkManager {}

impl NetworkManager {
  pub async fn new(daemon: SharedDaemon) -> Result<Arc<Mutex<Self>>, Box<dyn Error>> {
    let _ = tracing_subscriber::fmt().with_env_filter(EnvFilter::from_default_env()).try_init();
    let (p_sender, mut p_receiver) = tokio::sync::mpsc::channel(100);

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
        tokio::select! {
          event = swarm.select_next_some() => match event {
            SwarmEvent::NewListenAddr { address, ..} => {
              info!("Multiconnect listing on {:?}", address);
            }
            SwarmEvent::Behaviour(MulticonnectBehaviorEvent::Mnds(mdns::Event::Discovered(discoverd))) => {
              for (peer_id, multiaddr) in discoverd {
                info!("Discoverd peer: id = {}, multiaddr = {}", peer_id, multiaddr);
                let peer = Peer { peer_id, multiaddr };
                let _ = p_sender.send(Packet::PeerFound(PeerFound::new(peer))).await;
              }
            }
            SwarmEvent::Behaviour(MulticonnectBehaviorEvent::Mnds(mdns::Event::Expired(expired))) => {
              for (peer_id, multiaddr) in expired {
                info!("Expired peer: id = {}, multiaddr = {}", peer_id, multiaddr);
                let peer = Peer { peer_id, multiaddr };
                let _ = p_sender.send(Packet::PeerExpired(PeerExpired::new(peer))).await; // TODO: Expire peers
              }
            }
            SwarmEvent::Behaviour(MulticonnectBehaviorEvent::Pairing(request_response::Event::Message { peer, connection_id, message })) => {
              match message {
                request_response::Message::Request { request_id, request, channel } => {
                  let _ = p_sender.send(Packet::PeerPairRequest(request)).await;
                  // let accepted = true;
                  // let _ = swarm.behaviour_mut().pairing.send_response(channel, PairingResponse(accepted));

                  // if accepted {
                  //   info!("Peer paired successfully!");
                  // }

                },
                request_response::Message::Response { request_id, response } => {
                  info!("Received paring response: id = {}, res = {:?}", request_id, response);
                },
              }
            }
            _ => {
              trace!("Event: {:?}", event);
            }
          }
        }
      }
    });

    let daemon_task: tokio::task::JoinHandle<()> = tokio::spawn(async move {
      while let Some(packet) = p_receiver.recv().await {
        daemon.add_to_queue(packet).await;
      }
    });

    Ok(Arc::new(Mutex::new(Self {})))
  }
}

#[cfg(test)]
mod tests {
  use super::*;
}
