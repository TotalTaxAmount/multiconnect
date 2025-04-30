pub mod discovery;
pub mod pairing;

use async_trait::async_trait;
use libp2p::{
  request_response::{Event, Message, ResponseChannel},
  PeerId,
};
use log::debug;
use multiconnect_protocol::{Device, Packet, Peer};
use std::{collections::HashMap, sync::Arc};
use tokio::{
  sync::{broadcast, mpsc, oneshot, Mutex},
  time::{self, Duration},
};
use uid::Id;

use crate::{networking::NetworkManager, SharedDaemon};

#[derive(Debug, PartialEq)]
pub enum Target {
  Local(Packet),
  Peer(PeerId, Packet),
}

/// A module that can be used for features
#[async_trait]
pub trait MulticonnectModule: Send + Sync {
  /// Runs every 20ms, used for background tasks/other stuff service is doing
  async fn periodic(&mut self, ctx: &mut MulticonnectCtx);
  /// Runs when the swarm recivies a packet from another peer
  async fn on_peer_packet(&mut self, packet: Packet, source: PeerId, ctx: &mut MulticonnectCtx);
  /// Runs when the daemon recives a packet from the frontend
  async fn on_frontend_packet(&mut self, packet: Packet, ctx: &mut MulticonnectCtx);
  /// Runs once when the module is started
  async fn init(&mut self, ctx: Arc<Mutex<MulticonnectCtx>>);
}

/// Context that modules get and allows them to do things like send packets to the
/// frontend/peers and see what devices and currently paired
pub struct MulticonnectCtx {
  /// TA channel to send packets to various locations
  send_packet_tx: mpsc::Sender<Target>,
  /// A HashMap of PeerIds and corrosponding Devices and weather they are paired or not
  devices: HashMap<PeerId, (Device, bool)>,
  /// The current device
  this_device: Device,
}

impl MulticonnectCtx {
  /// Create a new instance of `MulticonnectCtx` <br />
  pub fn new(send_packet_tx: mpsc::Sender<Target>, this_device: Device) -> Self {
    Self { send_packet_tx, this_device, devices: HashMap::new() }
  }

  /// Send a packet to the frontend
  pub async fn send_to_frontend(&self, packet: Packet) {
    let _ = self.send_packet_tx.send(Target::Local(packet)).await;
  }

  /// Send a packet to a peer
  pub async fn send_to_peer(&self, target: PeerId, packet: Packet) {
    let _ = self.send_packet_tx.send(Target::Peer(target, packet)).await;
  }

  /// Get the HashMap of paired devices
  pub fn get_devices(&self) -> &HashMap<PeerId, (Device, bool)> {
    &self.devices
  }

  /// Get a paired device
  pub fn get_device(&self, id: &PeerId) -> Option<&(Device, bool)> {
    self.devices.get(id)
  }

  pub fn get_device_mut(&mut self, id: &PeerId) -> Option<&mut (Device, bool)> {
    self.devices.get_mut(id)
  }
  /// Add a device to the list of paired devices
  pub fn add_device(&mut self, device: Device) {
    self.devices.insert(device.peer, (device, false));
  }

  /// Remove a paired  device
  pub fn remove_device(&mut self, id: &PeerId) -> Option<(Device, bool)> {
    self.devices.remove(id)
  }

  /// Get the local peer id
  pub fn get_this_device(&self) -> &Device {
    &self.this_device
  }
}

/// Struct to manage all modules
pub struct ModuleManager {
  /// A list of all registered modules
  modules: Vec<Box<dyn MulticonnectModule>>,
  /// Pending requests
  pending_requests: HashMap<Id<Target>, ResponseChannel<Packet>>,
  /// Reciver for packets coming from the frontend
  recv_frontend_packet_rx: broadcast::Receiver<Packet>,
  /// Reciver for packets coming from peers
  recv_peer_packet_rx: broadcast::Receiver<(PeerId, Packet)>,
  /// Sender for peer related packets
  send_peer_packet_tx: mpsc::Sender<(PeerId, Packet)>,
  // Sender from sending frontend packets
  send_frontend_packet_tx: mpsc::Sender<Packet>,
  /// Packets to send from modules
  send_packet_rx: mpsc::Receiver<Target>,
  /// Context that is shared between all modules
  ctx: Arc<Mutex<MulticonnectCtx>>,
}

impl ModuleManager {
  /// Create a new module manager
  pub fn new(network_manager: NetworkManager, daemon: SharedDaemon) -> Self {
    let (send_packet_tx, send_packet_rx) = mpsc::channel(100);
    Self {
      ctx: Arc::new(Mutex::new(MulticonnectCtx::new(
        send_packet_tx,
        Device::this(network_manager.get_local_peer_id()),
      ))),
      modules: Vec::new(),
      pending_requests: HashMap::new(),
      send_frontend_packet_tx: daemon.send_packet_channel(),
      send_peer_packet_tx: network_manager.send_packet_channel(),
      recv_frontend_packet_rx: daemon.recv_packet_channel(),
      recv_peer_packet_rx: network_manager.recv_packet_channel(),
      send_packet_rx,
    }
  }

  /// Register a module
  pub fn register<M: MulticonnectModule + 'static>(&mut self, module: M) {
    let boxed = Box::new(module);
    self.modules.push(boxed);
  }

  /// Calls the `perodic`` function of every registerd module
  pub async fn call_perodic(&mut self, ctx: &mut MulticonnectCtx) {
    for module in self.modules.iter_mut() {
      module.periodic(ctx).await;
    }
  }

  /// Calls the `on_peer_packet` function of every register module
  pub async fn call_on_peer_packet(&mut self, id: PeerId, packet: Packet, ctx: &mut MulticonnectCtx) {
    for module in self.modules.iter_mut() {
      module.on_peer_packet(packet.clone(), id, ctx).await;
    }
  }

  /// Calls the `on_frontend_packet` function of every register module
  pub async fn call_on_frontend_packet(&mut self, packet: Packet, ctx: &mut MulticonnectCtx) {
    for module in self.modules.iter_mut() {
      module.on_frontend_packet(packet.clone(), ctx).await;
    }
  }

  pub async fn start(&'static mut self) {
    let ctx = self.ctx.clone();
    let mut periodic_interval = time::interval(Duration::from_millis(20));
    for module in self.modules.iter_mut() {
      module.init(ctx.clone()).await;
    }

    tokio::spawn(async move {
      loop {
        tokio::select! {
          event = self.recv_frontend_packet_rx.recv() => if let Ok(packet) = event {
            debug!("Calling on_frontend_packet");
            let mut ctx = ctx.lock().await;
            self.call_on_frontend_packet(packet, &mut ctx).await;
          },

          event = self.recv_peer_packet_rx.recv() => if let Ok((source, packet)) = event {
            debug!("Calling on_peer_packet");
            let mut ctx = ctx.lock().await;
            self.call_on_peer_packet(source, packet, &mut ctx).await;
          },

          target = self.send_packet_rx.recv() => if let Some(target) = target {

            match target {
              Target::Local(packet) => { let _ = self.send_frontend_packet_tx.send(packet.to_owned()).await; },
              Target::Peer(peer_id, packet) => { let _ = self.send_peer_packet_tx.send((peer_id, packet)).await; },
            };
          },
          _ = periodic_interval.tick() => {
            let mut ctx = ctx.lock().await;
            self.call_perodic(&mut ctx).await;
          }
        }
      }
    });
  }
}
