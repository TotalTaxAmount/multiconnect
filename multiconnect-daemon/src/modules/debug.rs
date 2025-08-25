use std::{error::Error, str::FromStr, sync::Arc};

use async_trait::async_trait;
use libp2p::PeerId;
use multiconnect_core::{p2p::P0Ping, Packet};
use tokio::sync::Mutex;

use crate::{
  modules::{MulticonnectCtx, MulticonnectModule},
  networking::NetworkEvent,
  FrontendEvent,
};

pub struct DebugModule {}

impl DebugModule {
  pub fn new() -> Self {
    Self {}
  }
}

#[async_trait]
impl MulticonnectModule for DebugModule {
  async fn on_frontend_event(&mut self, event: FrontendEvent, ctx: &mut MulticonnectCtx) -> Result<(), Box<dyn Error>> {
    match event {
      FrontendEvent::RecvPacket(packet) =>
        if let Packet::D0Debug(debug) = packet {
          let (cmd, peer_id) = debug.debug.split_once(':').unwrap();
          let peer_id = PeerId::from_str(peer_id).unwrap();
          match cmd {
            "open-stream" => ctx.open_stream(peer_id).await,
            "send-ping" => ctx.send_to_peer(&peer_id, Packet::P0Ping(P0Ping::new())).await,
            "close-stream" => ctx.close_stream(peer_id).await,
            _ => {}
          };
        },
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
