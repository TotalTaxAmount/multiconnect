use std::str::FromStr;

use async_trait::async_trait;
use bincode::de;
use libp2p::PeerId;
use log::debug;
use multiconnect_protocol::{local::peer::L0PeerFound, shared::peer::S1PeerMeta, Device, Packet, Peer};

use super::{MulticonnectCtx, MulticonnectModule};

pub struct Discovery;

#[async_trait]
impl MulticonnectModule for Discovery {
  #[doc = " Runs every 20ms, used for background tasks/other stuff service is doing"]
  async fn periodic(&mut self, _ctx: &mut MulticonnectCtx) {}

  #[doc = " Runs when the swarm recivies a packet from another peer"]
  async fn on_peer_packet(&mut self, packet: Packet, source: PeerId, ctx: &mut MulticonnectCtx) {
    match packet {
      Packet::S1PeerMeta(packet) => {
        let device = Device::from_meta(packet, source);
        debug!("Received meta for device: {:?}", device);

        if ctx.get_this_device().peer < source {
          debug!("Sending metadata to: {}", source);
          ctx.send_to_peer(source, Packet::S1PeerMeta(S1PeerMeta::from_device(ctx.get_this_device()))).await;
        }

        ctx.add_device(device.clone());
        ctx.send_to_frontend(Packet::L0PeerFound(L0PeerFound::new(&device))).await;
      }
      Packet::L1PeerExpired(packet) => {
        ctx.remove_device(&PeerId::from_str(&packet.peer_id).unwrap());
        ctx.send_to_frontend(Packet::L1PeerExpired(packet)).await;
      }
      _ => {}
    }
  }

  #[doc = " Runs when the daemon recives a packet from the frontend"]
  async fn on_frontend_packet(&mut self, packet: Packet, ctx: &mut MulticonnectCtx) {
    match packet {
      Packet::L4Refresh(_) => {
        for (_, (device, _)) in ctx.get_devices().iter() {
          ctx.send_to_frontend(Packet::L0PeerFound(L0PeerFound::new(device))).await;
        }
      }
      _ => {}
    }
  }
}
