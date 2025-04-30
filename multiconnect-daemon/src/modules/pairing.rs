use std::{collections::HashMap, str::FromStr, sync::Arc};

use async_trait::async_trait;
use libp2p::{request_response::Message, PeerId};
use log::{debug, info};
use multiconnect_protocol::{
  local::peer::{L2PeerPairRequest, L3PeerPairResponse},
  p2p::peer::{P2PeerPairRequest, P3PeerPairResponse},
  Device, Packet,
};
use tokio::{
  sync::{mpsc, Mutex},
  time::Instant,
};
use uuid::Uuid;

use crate::networking::PairingProtocolEvent;

use super::{MulticonnectCtx, MulticonnectModule};

pub struct PairingModule {
  pending_requests: HashMap<Uuid, (Instant, Packet, PeerId)>,
  pairing_protocol_send: mpsc::Sender<PairingProtocolEvent>,
  pairing_protocol_recv: Option<mpsc::Receiver<PairingProtocolEvent>>,
}

impl PairingModule {
  pub fn new(
    pairing_protocol_send: mpsc::Sender<PairingProtocolEvent>,
    pairing_protocol_recv: mpsc::Receiver<PairingProtocolEvent>,
  ) -> Self {
    Self { pending_requests: HashMap::new(), pairing_protocol_recv: Some(pairing_protocol_recv), pairing_protocol_send }
  }
}

#[async_trait]
impl MulticonnectModule for PairingModule {
  #[doc = " Runs every 20ms, used for background tasks/other stuff service is doing"]
  async fn periodic(&mut self, _ctx: &mut MulticonnectCtx) {
    self.pending_requests.retain(|_, (instant, _, _)| instant.elapsed().as_secs() < 60);
  }

  #[doc = " Runs when the swarm recivies a packet from another peer"]
  async fn on_peer_packet(&mut self, packet: Packet, source: PeerId, ctx: &mut MulticonnectCtx) {
    match packet {
      Packet::P2PeerPairRequest(peer_pair_request) => {
        info!("Received pairing request from {:?}, req_id = {}", source, peer_pair_request.req_uuid);
        let uuid = Uuid::from_str(&peer_pair_request.req_uuid).unwrap();
        let device = bincode::deserialize::<Device>(&peer_pair_request.device).unwrap();
        self
          .pending_requests
          .insert(uuid, (Instant::now(), Packet::P2PeerPairRequest(peer_pair_request.clone()), source));

        ctx.send_to_frontend(Packet::L2PeerPairRequest(L2PeerPairRequest::new(&device, uuid))).await;
      }
      Packet::P3PeerPairResponse(peer_pair_response) => {
        info!(
          "Received paring response: id = {}, res = {:?}",
          peer_pair_response.req_uuid, peer_pair_response.accepted
        );
        let uuid = Uuid::from_str(&peer_pair_response.req_uuid).unwrap();
        debug!("uuid = {}, pending = {:?}", uuid, self.pending_requests);
        if let Some((_, Packet::L2PeerPairRequest(req), _)) = self.pending_requests.remove(&uuid) {
          debug!("Found a valid request for a response");
          if peer_pair_response.accepted {
            let device = bincode::deserialize::<Device>(&req.device).unwrap();
            if let Some((_, paired)) = ctx.get_device_mut(&device.peer) {
              info!("Successfully paired with: {}", device.peer);
              *paired = true;
            }
            ctx
              .send_to_frontend(Packet::L3PeerPairResponse(L3PeerPairResponse::new(peer_pair_response.accepted, uuid)))
              .await;
          } else {
            info!("Pairing request denied")
          }
        }
      }
      _ => {}
    }
  }

  #[doc = " Runs when the daemon recives a packet from the frontend"]
  async fn on_frontend_packet(&mut self, packet: Packet, ctx: &mut MulticonnectCtx) {
    match packet {
      Packet::L2PeerPairRequest(packet) => {
        let device = bincode::deserialize::<Device>(&packet.device).unwrap(); // Get the target peer to send the request to from the frontend
        debug!("Sending pair request to: {}, id = {}", device.peer, packet.req_uuid);
        let uuid = Uuid::from_str(&packet.req_uuid).unwrap();
        self
          .pending_requests
          .insert(uuid, (Instant::now(), Packet::L2PeerPairRequest(packet.clone()), ctx.get_this_device().peer)); // Add the request to pending requests so we know if we get a response
        ctx.send_to_peer(device.peer, Packet::P2PeerPairRequest(P2PeerPairRequest::new(&device, uuid))).await;
        // Send the request to the peer
      }
      Packet::L3PeerPairResponse(packet) => {
        let uuid = Uuid::from_str(&packet.req_uuid).unwrap();
        // Check if there is a pending request for that id
        if let Some((_, Packet::P2PeerPairRequest(_req), source)) = self.pending_requests.remove(&uuid) {
          debug!("Sending back response");
          // Send the response
          let _ =
            ctx.send_to_peer(source, Packet::P3PeerPairResponse(P3PeerPairResponse::new(uuid, packet.accepted))).await;
        }
      }
      _ => {}
    }
  }

  async fn init(&mut self, ctx: Arc<Mutex<MulticonnectCtx>>) {
    if let Some(ch) = self.pairing_protocol_recv.take() {
      let ctx = ctx.clone();
      tokio::spawn(async move {});
    }
  }
}
