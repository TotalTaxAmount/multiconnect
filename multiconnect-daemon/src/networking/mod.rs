mod pairing;
mod store;

use std::{error::Error, sync::Arc, time::Duration};

use libp2p::{
  futures::StreamExt, mdns, noise, request_response::{self, Config, ProtocolSupport}, swarm::{NetworkBehaviour, SwarmEvent}, tcp, yamux, Multiaddr, SwarmBuilder
};
use log::{debug, error, info, trace};
use multiconnect_protocol::{
  peer::{PeerExpired, PeerFound, PeerPairRequest},
  Packet, Peer,
};
use pairing::PairingCodec;
use tokio::sync::{Mutex, RwLock};
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
  pub async fn start(daemon: SharedDaemon) -> Result<(), Box<dyn Error>> {
    let _ = tracing_subscriber::fmt().with_env_filter(EnvFilter::from_default_env()).try_init();

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

    let _ = tokio::spawn(async move {
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
                daemon.send_packet(Packet::PeerFound(PeerFound::new(peer))).await;
                // let _ = p_sender.send(Packet::PeerFound(PeerFound::new(peer))).await;
              }
            }
            SwarmEvent::Behaviour(MulticonnectBehaviorEvent::Mnds(mdns::Event::Expired(expired))) => {
              for (peer_id, multiaddr) in expired {
                info!("Expired peer: id = {}, multiaddr = {}", peer_id, multiaddr);
                let peer = Peer { peer_id, multiaddr };
                daemon.send_packet(Packet::PeerExpired(PeerExpired::new(peer))).await
                // let _ = p_sender.send(Packet::PeerExpired(PeerExpired::new(peer))).await;
              }
            }
            SwarmEvent::Behaviour(MulticonnectBehaviorEvent::Pairing(request_response::Event::Message { peer, connection_id, message })) => {
              match message {
                request_response::Message::Request { request_id: _, request, channel: _ } => {
                  debug!("Received pairing request from {:?}", bincode::deserialize::<Peer>(&request.peer).unwrap());
                  // let _ = p_sender.send(Packet::PeerPairRequest(request)).await;
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

    // let _ = tokio::spawn(async move {
    //   loop {
    //     let packet: Option<Packet>;
    //     {
    //       let mut locked = daemon_cloned.lock().await;
    //       packet = locked.on_packet().await;
    //     }

    //     match packet {
    //       Some(Packet::PeerPairRequest(packet)) => {
    //         debug!("Sending pair request");
    //         let peer = bincode::deserialize::<Peer>(&packet.peer).unwrap();
    //         // swarm.behaviour_mut().pairing.send_request(&peer.peer_id, packet);
    //       },
    //       Some(_) | None => {},
    //     }
    //   }()
    // });
    Ok(())
  }
}

#[cfg(test)]
mod tests {
  use super::*;
}
