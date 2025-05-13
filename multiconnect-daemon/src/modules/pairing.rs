use std::{collections::HashMap, str::FromStr, sync::Arc, time::Duration};

use async_trait::async_trait;
use bincode::de;
use libp2p::{request_response::ResponseChannel, PeerId};
use log::{debug, info, warn};
use multiconnect_protocol::{
  local::peer::{L0PeerFound, L1PeerExpired, L2PeerPairRequest, L3PeerPairResponse, L8SavedPeerUpdate},
  p2p::peer::{P2PeerPairRequest, P3PeerPairResponse},
  shared::peer::S1PeerMeta,
  Device, Packet,
};
use tokio::{
  sync::{mpsc, Mutex, RwLock},
  time::{interval, Instant},
};
use uuid::Uuid;

use crate::{
  networking::{NetworkCommand, NetworkEvent, PairingProtocolEvent},
  FrontendEvent,
};

use super::{MulticonnectCtx, MulticonnectModule};

/// Pairing Module:
/// Handles discovery and pairing
/// Also opens streams to already paired peers and decides weather or not to
/// accept a inbound stream
///
/// **Discovery**:
///   - Peer is discoverd on network via mDNS
///   - Lexicographically decide which peer should send pairing protocol request
///     containing a S1PeerMeta of itself
///   - Other peer receives S1PeerMeta and adds the device to it's discoverd
///     devices
///   - Other peer sends a response with a S1PeerMeta of itself
///   - Sender adds other device to discoverd devices
///
/// **Pairing**:
///  - *Sending*:
///     - Receives a L2PeerPairRequest from the frontend
///     - Add the request to pending requests and send a P2PeerPairRequest to the
///       target peer
///     - Receive a P3PeerPairResponse from a peer and match it to a request
///     - If its accepted add it it to paired peers (TODO: Also save to disk to
///       auto pair later)
///     - Send result to frontend
///  - *Receiving*:
///     - Receive a P2PairRequest and add it to pending requests
///     - Send a L2PeerPairRequest to the frontend
///     - Get a response from the frontend (L3PeerPairResponse)
///     - If accepted save to paired peers (and write to disk)
///     - Send result to other peer
pub struct PairingModule {
  /// Pending pair requests
  pending_requests: Arc<Mutex<HashMap<Uuid, (Instant, Packet, PeerId)>>>,
  /// Response channels for requests
  res_channels: Arc<Mutex<HashMap<PeerId, (Instant, ResponseChannel<Packet>)>>>,
  /// A channel for sending network commands
  pairing_protocol_send: mpsc::Sender<NetworkCommand>,
  /// A channel for receiving pairing protocol events
  pairing_protocol_recv: Option<mpsc::Receiver<PairingProtocolEvent>>,
}

impl PairingModule {
  pub async fn new(
    pairing_protocol_send: mpsc::Sender<NetworkCommand>,
    pairing_protocol_recv: mpsc::Receiver<PairingProtocolEvent>,
  ) -> Self {
    Self {
      pending_requests: Arc::new(Mutex::new(HashMap::new())),
      pairing_protocol_recv: Some(pairing_protocol_recv),
      pairing_protocol_send,
      res_channels: Arc::new(Mutex::new(HashMap::new())),
    }
  }
}

#[async_trait]
impl MulticonnectModule for PairingModule {
  #[doc = " Runs every 20ms, used for background tasks/other stuff service is doing"]
  async fn periodic(&mut self, _ctx: &mut MulticonnectCtx) {}

  #[doc = " Runs when the swarm recivies a packet from another peer"]
  async fn on_network_event(&mut self, event: NetworkEvent, ctx: &mut MulticonnectCtx) {
    match event {
      NetworkEvent::PeerExpired(peer_id) => {
        if let Some((_, Some(true))) = ctx.get_device(&peer_id) {
          ctx.send_to_frontend(Packet::L8SavedPeerUpdate(L8SavedPeerUpdate::update_online(&peer_id, false))).await;
        } else {
          ctx.send_to_frontend(Packet::L1PeerExpired(L1PeerExpired::new(&peer_id))).await;
        }
      }
      NetworkEvent::PeerDiscoverd(peer_id) => {
        if let Some(_) = ctx.get_device(&peer_id) {
          ctx.send_to_frontend(Packet::L8SavedPeerUpdate(L8SavedPeerUpdate::update_online(&peer_id, true))).await;
        }
        if ctx.this_device.peer > peer_id {
          debug!("[first] Sending metadata to {}", peer_id);
          let _ = self
            .pairing_protocol_send
            .send(NetworkCommand::SendPairingProtocolRequest(
              peer_id,
              Packet::S1PeerMeta(S1PeerMeta::from_device(&ctx.this_device)),
            ))
            .await;
        }
      }
      NetworkEvent::ConnectionOpenRequest(peer_id) => {
        if let Some((_, paired)) = ctx.get_device(&peer_id) {
          debug!("Received connection request from {} (saved: {:?})", peer_id, paired);
          if Some(true) == *paired {
            ctx.approve_inbound_stream(peer_id).await;
          } else {
            ctx.deny_inbound_stream(peer_id).await;
          }
        }
      }
      _ => {}
    };
  }

  #[doc = " Runs when the daemon recives a packet from the frontend"]
  async fn on_frontend_event(&mut self, event: FrontendEvent, ctx: &mut MulticonnectCtx) {
    match event {
      FrontendEvent::RecvPacket(packet) => match packet {
        Packet::L3PeerPairResponse(packet) => {
          let uuid = Uuid::from_str(&packet.req_uuid).unwrap();
          // Check if there is a pending request for that id
          if let Some((_, Packet::P2PeerPairRequest(_req), source)) = self.pending_requests.lock().await.remove(&uuid) {
            if let Some((_, ch)) = self.res_channels.lock().await.remove(&source) {
              debug!("Sending back response");
              // Send the response
              let _ = self
                .pairing_protocol_send
                .send(NetworkCommand::SendPairingProtocolResponse(
                  ch,
                  Packet::P3PeerPairResponse(P3PeerPairResponse::new(uuid, packet.accepted)),
                ))
                .await;
            } else {
              warn!("Failed to find response channel for request")
            }
          }
        }
        Packet::L2PeerPairRequest(packet) => {
          let device: Device = bincode::deserialize::<Device>(&packet.device).unwrap();
          if ctx.device_exists(&device.peer) {
            debug!("Sending pair request to: {}, id = {}", device.peer, packet.req_uuid);
            let uuid = Uuid::from_str(&packet.req_uuid).unwrap();
            self
              .pending_requests
              .lock()
              .await
              .insert(uuid, (Instant::now(), Packet::L2PeerPairRequest(packet.clone()), ctx.get_this_device().peer));
            let _ = self
              .pairing_protocol_send
              .send(NetworkCommand::SendPairingProtocolRequest(
                device.peer,
                Packet::P2PeerPairRequest(P2PeerPairRequest::new(&device, uuid)),
              ))
              .await;
          }
        }
        Packet::L4Refresh(_) => {
          let devices = ctx.get_devices().values();
          for (device, _) in devices {
            let _ = ctx.send_to_frontend(Packet::L0PeerFound(L0PeerFound::new(device))).await;
          }
        }
        _ => {}
      },

      FrontendEvent::Connected => {}
      _ => {}
    }
  }

  async fn init(&mut self, ctx: Arc<Mutex<MulticonnectCtx>>) {
    if let Some(mut ch) = self.pairing_protocol_recv.take() {
      let pairing_protocol_send = self.pairing_protocol_send.clone();
      let pending_requests = self.pending_requests.clone();
      let res_channels = self.res_channels.clone();
      let mut retain_interval = interval(Duration::from_secs(10));

      {
        let guard = ctx.lock().await;
        let devices = guard.get_devices();
        for (peer_id, (_, paired)) in devices.iter() {
          if Some(true) == *paired && guard.get_this_device().peer > *peer_id {
            let _ = pairing_protocol_send.send(NetworkCommand::OpenStream(*peer_id)).await;
          }
        }
      }

      tokio::spawn(async move {
        loop {
          tokio::select! {
            event = ch.recv() => if let Some(event) = event {
              match event {
                  PairingProtocolEvent::RecvRequest(peer_id, packet, response_channel) => {
                    match packet {
                      Packet::S1PeerMeta(packet) => {
                        let device = Device::from_meta(packet, peer_id);
                        debug!("Revived device meta {:?}", device);

                        let mut guard = ctx.lock().await;
                        guard.send_to_frontend(Packet::L0PeerFound(L0PeerFound::new(&device))).await;
                        guard.add_device(device);

                        debug!("[second] Sending meta to {}", peer_id);
                        let _ = pairing_protocol_send.send(NetworkCommand::SendPairingProtocolResponse(response_channel, Packet::S1PeerMeta(S1PeerMeta::from_device(guard.get_this_device())))).await;

                      },
                      Packet::P2PeerPairRequest(packet) => {
                        info!("Received pairing request from {}, req_id = {}", peer_id, packet.req_uuid);

                        let uuid = Uuid::from_str(&packet.req_uuid).unwrap();
                        let device = bincode::deserialize::<Device>(&packet.device).unwrap();

                        pending_requests.lock().await.insert(uuid, (Instant::now(), Packet::P2PeerPairRequest(packet.clone()), peer_id));
                        res_channels.lock().await.insert(peer_id, (Instant::now(), response_channel));

                        let guard = ctx.lock().await;
                        guard.send_to_frontend(Packet::L2PeerPairRequest(L2PeerPairRequest::new(&device, uuid))).await;
                      },
                      _ => {
                        warn!("Unexpected packet received");
                      }
                    }
                  },
                  PairingProtocolEvent::RecvResponse(peer_id, packet) => {
                    match packet {
                      Packet::S1PeerMeta(packet) => {
                        let device = Device::from_meta(packet, peer_id);
                        debug!("Received device meta: {:?}", device);
                        let mut guard = ctx.lock().await;
                        guard.send_to_frontend(Packet::L0PeerFound(L0PeerFound::new(&device))).await;
                        guard.add_device(device);

                      },
                      Packet::P3PeerPairResponse(packet) => {
                        info!("Received paring response: uuid = {}, accepted = {}", packet.req_uuid, packet.accepted);
                        let uuid = Uuid::from_str(&packet.req_uuid).unwrap();
                        if let Some((_, Packet::L2PeerPairRequest(req), _)) = pending_requests.lock().await.remove(&uuid) {
                          debug!("Found request for response");
                          let mut guard = ctx.lock().await;
                          if packet.accepted {
                            let device = bincode::deserialize::<Device>(&req.device).unwrap();
                            if let Some((_, paired)) = guard.get_device_mut(&device.peer) {
                              info!("Successfully paired with: {}", device.peer);
                              *paired = Some(true);
                              guard.save_store().await;
                            }
                          }

                          guard.send_to_frontend(Packet::L3PeerPairResponse(L3PeerPairResponse::new(packet.accepted, uuid))).await;
                        }
                      },
                      _ => {
                        warn!("Unexpected packet received");
                      },
                    }
                  },
              }
            },

            _ = retain_interval.tick() => {
              pending_requests.lock().await.retain(|_, (instant, _, _)| instant.elapsed().as_secs() > 60);
              res_channels.lock().await.retain(|_, (instant, _)| instant.elapsed().as_secs() > 30);
            }
          }
        }
      });
    }
  }
}
