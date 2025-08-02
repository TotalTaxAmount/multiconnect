use std::{collections::HashMap, error::Error, str::FromStr, sync::Arc, time::Duration};

use async_trait::async_trait;
use libp2p::{request_response::ResponseChannel, PeerId};
use log::{debug, info, warn};
use multiconnect_protocol::{
  local::peer::{
    L0PeerFound, L1PeerExpired, L2PeerPairRequest, L3PeerPairResponse, L7DeviceStatus, L8DeviceStatusUpdate,
  },
  p2p::{
    peer::{P2PeerPairRequest, P3PeerPairResponse},
    P0Ping,
  },
  shared::peer::S1PeerMeta,
  Device, Packet, SavedDevice,
};
use tokio::{
  sync::{mpsc, Mutex},
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
///     - Add the request to pending requests and send a P2PeerPairRequest to
///       the target peer
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
  ///
  discovered_devices: Arc<Mutex<HashMap<PeerId, Device>>>,
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
      discovered_devices: Arc::new(Mutex::new(HashMap::new())),
      pairing_protocol_send,
      res_channels: Arc::new(Mutex::new(HashMap::new())),
    }
  }
}

#[async_trait]
impl MulticonnectModule for PairingModule {
  #[doc = " Runs when the swarm receives a packet from another peer"]
  async fn on_network_event(&mut self, event: NetworkEvent, ctx: &mut MulticonnectCtx) -> Result<(), Box<dyn Error>> {
    match event {
      NetworkEvent::PeerExpired(peer_id) => {
        if let Some((_, online, _)) = ctx.get_device_mut(&peer_id) {
          *online = false;
          ctx
            .send_to_frontend(Packet::L8DeviceStatusUpdate(L8DeviceStatusUpdate::update_online(&peer_id, false)))
            .await;
        } else {
          ctx.send_to_frontend(Packet::L1PeerExpired(L1PeerExpired::new(&peer_id))).await;
        }
      }
      NetworkEvent::PeerDiscoverd(peer_id) => {
        if let Some((_, online, _)) = ctx.get_device_mut(&peer_id) {
          *online = true;
          ctx.send_to_frontend(Packet::L8DeviceStatusUpdate(L8DeviceStatusUpdate::update_online(&peer_id, true))).await;

          // if ctx.this_device.peer > peer_id {
          //   ctx.dial_peer(peer_id).await;
          // }
        } else {
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
      }

      _ => {}
    };

    Ok(())
  }

  #[doc = " Runs when the daemon receives a packet from the frontend"]
  async fn on_frontend_event(&mut self, event: FrontendEvent, ctx: &mut MulticonnectCtx) -> Result<(), Box<dyn Error>> {
    match event {
      FrontendEvent::RecvPacket(packet) => match packet {
        // This happens on the reciviers system
        Packet::L3PeerPairResponse(packet) => {
          let uuid = Uuid::from_str(&packet.req_uuid)?;
          debug!("Received response for: {}", uuid);
          let mut pending_requests = self.pending_requests.lock().await;
          // Check if there is a pending request for that id
          if let Some((_, Packet::P2PeerPairRequest(_req), source)) = pending_requests.remove(&uuid) {
            if let Some((_, ch)) = self.res_channels.lock().await.remove(&source) {
              // let device = bincode::deserialize::<Device>(&req.device)?; // <- The target

              if packet.accepted {
                info!("Saving paired device");
                if let Some(source_device) = self.discovered_devices.lock().await.remove(&source) {
                  ctx.add_device(SavedDevice::new(source_device, true));
                  ctx.save_store().await;
                  ctx.update_whitelist(source, true).await;
                }

                // debug!("Attempting to open stream");
                // ctx.open_stream(source).await;
              }

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
              warn!("Failed to find response channel for request");
            }
          } else {
            warn!("Invalid request uuid: pending = {:?}", pending_requests);
          }
        }
        Packet::L2PeerPairRequest(packet) => {
          let target: Device = bincode::deserialize::<Device>(&packet.device)?;
          let uuid = Uuid::from_str(&packet.req_uuid)?;

          if self.discovered_devices.lock().await.contains_key(&target.peer) && !ctx.device_exists(&target.peer) {
            debug!("Sending pair request to: {}, id = {}", target.peer, packet.req_uuid);
            self
              .pending_requests
              .lock()
              .await
              //      Request uuid | Timestamp | request packet | source
              .insert(uuid, (Instant::now(), Packet::L2PeerPairRequest(packet.clone()), ctx.get_this_device().peer));
            let _ = self
              .pairing_protocol_send
              .send(NetworkCommand::SendPairingProtocolRequest(
                target.peer,
                Packet::P2PeerPairRequest(P2PeerPairRequest::new(&target, uuid)),
              ))
              .await;
          } else {
            debug!("Devices already pairied or doesnt exist");
            let _ = ctx.send_to_frontend(Packet::L3PeerPairResponse(L3PeerPairResponse::new(false, uuid)));
          }
        }
        Packet::L4Refresh(_) => {
          let paired_devices = ctx.get_devices().values();
          for (device, online, _connected) in paired_devices {
            ctx
              .send_to_frontend(Packet::L7DeviceStatus(L7DeviceStatus::new(
                device.get_device().peer,
                *online,
                device.get_device(),
                device.last_seen(),
              )))
              .await;
          }

          for device in self.discovered_devices.lock().await.values() {
            ctx.send_to_frontend(Packet::L0PeerFound(L0PeerFound::new(device))).await;
          }

          for (uuid, (_, packet, source)) in self.pending_requests.lock().await.iter() {
            if let Packet::L2PeerPairRequest(req) = packet {
              if source != &ctx.this_device.peer {
                let device = bincode::deserialize::<Device>(&req.device)?;
                ctx.send_to_frontend(Packet::L2PeerPairRequest(L2PeerPairRequest::new(&device, uuid.clone()))).await;
              }
            }
          }
        }
        _ => {}
      },
      FrontendEvent::Connected => {}
      _ => {}
    }

    Ok(())
  }

  async fn init(&mut self, ctx: Arc<Mutex<MulticonnectCtx>>) -> Result<(), Box<dyn Error>> {
    let mut ch = self.pairing_protocol_recv.take().unwrap();
    let pairing_protocol_send = self.pairing_protocol_send.clone();
    let discovered_devices = self.discovered_devices.clone();
    let pending_requests = self.pending_requests.clone();
    let res_channels = self.res_channels.clone();
    let mut retain_interval = interval(Duration::from_secs(10));

    {
      let ctx = ctx.lock().await;
      for (peer_id, (device, _, _)) in ctx.get_devices() {
        ctx.update_whitelist(*peer_id, device.is_paired()).await;
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

                      let guard = ctx.lock().await;
                      guard.send_to_frontend(Packet::L0PeerFound(L0PeerFound::new(&device))).await;
                      let mut discovered_devices = discovered_devices.lock().await;
                      discovered_devices.insert(device.peer, device);

                      debug!("[second] Sending meta to {}", peer_id);
                      let _ = pairing_protocol_send.send(NetworkCommand::SendPairingProtocolResponse(response_channel, Packet::S1PeerMeta(S1PeerMeta::from_device(guard.get_this_device())))).await;

                    },
                    // This happens on the reciviers system
                    Packet::P2PeerPairRequest(packet) => {
                      info!("Received pairing request from {}, req_id = {}", peer_id, packet.req_uuid);

                      let uuid = Uuid::from_str(&packet.req_uuid).unwrap();
                      let source = bincode::deserialize::<Device>(&packet.device).unwrap();
                      // if let Some(device) = discovered_devices.lock().await.get(&peer_id) {}

                      pending_requests.lock().await.insert(uuid, (Instant::now(), Packet::P2PeerPairRequest(packet.clone()), peer_id));
                      debug!("Pending: {:?}", pending_requests.lock().await);
                      res_channels.lock().await.insert(peer_id, (Instant::now(), response_channel));

                      let guard = ctx.lock().await;
                      guard.send_to_frontend(Packet::L2PeerPairRequest(L2PeerPairRequest::new(&source, uuid))).await;
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
                      let guard = ctx.lock().await;
                      guard.send_to_frontend(Packet::L0PeerFound(L0PeerFound::new(&device))).await;
                      let mut discovered_devices = discovered_devices.lock().await;
                      discovered_devices.insert(device.peer, device);
                    },
                    // This happens on the initators system
                    Packet::P3PeerPairResponse(packet) => {
                      info!("Received paring response: uuid = {}, accepted = {}", packet.req_uuid, packet.accepted);
                      let uuid = Uuid::from_str(&packet.req_uuid).unwrap();
                      let mut pending_requests = pending_requests.lock().await;
                      if let Some((_, Packet::L2PeerPairRequest(req), _)) = pending_requests.remove(&uuid) {
                        debug!("Found request for response");
                        let mut guard = ctx.lock().await;
                        if packet.accepted {
                          let device = bincode::deserialize::<Device>(&req.device).unwrap();
                          info!("Successfully paired with: {}", device.peer);

                          discovered_devices.lock().await.remove(&device.peer);
                          guard.add_device(SavedDevice::new(device.clone(), true));
                          guard.save_store().await;
                          guard.update_whitelist(device.peer, true).await;
                          guard.open_stream(device.peer).await;

                        }

                        guard.send_to_frontend(Packet::L3PeerPairResponse(L3PeerPairResponse::new(packet.accepted, uuid))).await;
                      } else {
                        warn!("Could not find a request with uuid = {}, pending = {:?}", uuid, pending_requests);
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
            pending_requests.lock().await.retain(|_, (instant, _, _)| instant.elapsed().as_secs() < 60);
            res_channels.lock().await.retain(|_, (instant, _)| instant.elapsed().as_secs() < 30);
          }
        }
      }
    });

    Ok(())
  }
}
