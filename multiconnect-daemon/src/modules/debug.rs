use std::{error::Error, str::FromStr, sync::Arc};

use async_trait::async_trait;
use libp2p::PeerId;
use log::debug;
use multiconnect_protocol::{p2p::P0Ping, Packet};
use tokio::sync::Mutex;

use crate::{
  modules::{MulticonnectCtx, MulticonnectModule},
  networking::NetworkEvent,
  FrontendEvent,
};

pub struct DebugModule {
  bruh: bool,
}

impl DebugModule {
  pub fn new() -> Self {
    Self { bruh: false }
  }
}

#[async_trait]
impl MulticonnectModule for DebugModule {
  async fn on_frontend_event(&mut self, event: FrontendEvent, ctx: &mut MulticonnectCtx) -> Result<(), Box<dyn Error>> {
    match event {
      FrontendEvent::RecvPacket(packet) => {
        if let Packet::L1PeerExpired(packet) = packet {
          let peer_id = PeerId::from_str(&packet.peer_id).unwrap();
          if !self.bruh {
            ctx.open_stream(peer_id).await;
            self.bruh = true;
          } else {
            debug!("Sending ping");
            ctx.send_to_peer(peer_id, Packet::P0Ping(P0Ping::new())).await;
          }
        }
      }
      _ => {}
    };

    Ok(())
  }

  async fn on_network_event(&mut self, event: NetworkEvent, ctx: &mut MulticonnectCtx) -> Result<(), Box<dyn Error>> {
    Ok(())
  }

  async fn init(&mut self, ctx: Arc<Mutex<MulticonnectCtx>>) -> Result<(), Box<dyn Error>> {
    Ok(())
  }
}
