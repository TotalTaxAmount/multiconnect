pub mod pairing;
pub mod store;

use async_trait::async_trait;
use libp2p::PeerId;
use log::{debug, error};
use multiconnect_protocol::{Device, Packet};
use std::{collections::HashMap, sync::Arc};
use store::Store;
use tokio::{
  sync::{mpsc, Mutex},
  time::{self, Duration},
};

use crate::{
  networking::{NetworkCommand, NetworkEvent, NetworkManager},
  FrontendEvent, SharedDaemon,
};

#[derive(Debug, PartialEq)]
pub enum Action {
  SendFrontend(Packet),
  SendPeer(PeerId, Packet),
  ApproveStream(PeerId),
  DenyStream(PeerId),
  OpenStream(PeerId),
  CloseStream(PeerId),
}

/// A module that can be used for features
#[async_trait]
pub trait MulticonnectModule: Send + Sync {
  /// Runs every 20ms, used for background tasks/other stuff service is doing
  async fn periodic(&mut self, ctx: &mut MulticonnectCtx);
  /// Runs when the swarm recivies a packet from another peer
  // async fn on_peer_packet(&mut self, packet: Packet, source: PeerId, ctx: &mut MulticonnectCtx);
  /// Runs when the daemon recives a packet from the frontend
  async fn on_frontend_event(&mut self, event: FrontendEvent, ctx: &mut MulticonnectCtx);
  /// Runs when a peer is discovered
  async fn on_network_event(&mut self, event: NetworkEvent, ctx: &mut MulticonnectCtx);
  // /// Runs when the frontend connects
  // async fn on_fronend_connected(&mut self, ctx: &mut MulticonnectCtx);
  /// Runs once when the module is started
  async fn init(&mut self, ctx: Arc<Mutex<MulticonnectCtx>>);
}

/// Context that modules get and allows them to do things like send packets to
/// the frontend/peers and see what devices and currently paired
pub struct MulticonnectCtx {
  /// TA channel to send packets to various locations
  action_tx: mpsc::Sender<Action>,
  /// The current device
  this_device: Device,
  /// Device store
  store: Store,
}

impl MulticonnectCtx {
  /// Create a new instance of `MulticonnectCtx` <br />
  pub async fn new(send_packet_tx: mpsc::Sender<Action>, this_device: Device) -> Self {
    let store = Store::new().await;
    Self { action_tx: send_packet_tx, this_device, store }
  }

  /// Send a packet to the frontend
  pub async fn send_to_frontend(&self, packet: Packet) {
    let _ = self.action_tx.send(Action::SendFrontend(packet)).await;
  }

  /// Send a packet to a peer
  pub async fn send_to_peer(&self, target: PeerId, packet: Packet) {
    let _ = self.action_tx.send(Action::SendPeer(target, packet)).await;
  }

  pub async fn approve_inbound_stream(&self, peer_id: PeerId) {
    let _ = self.action_tx.send(Action::ApproveStream(peer_id)).await;
  }

  pub async fn deny_inbound_stream(&self, peer_id: PeerId) {
    let _ = self.action_tx.send(Action::DenyStream(peer_id)).await;
  }

  pub async fn close_stream(&self, peer_id: PeerId) {
    let _ = self.action_tx.send(Action::CloseStream(peer_id)).await;
  }

  pub async fn save_store(&self) {
    self.store.save().await;
  }

  /// Get the HashMap of paired devices
  pub fn get_devices(&self) -> &HashMap<PeerId, (Device, Option<bool>)> {
    &self.store.get_saved_devices()
  }

  /// Get a paired device
  pub fn get_device(&self, id: &PeerId) -> Option<&(Device, Option<bool>)> {
    self.store.get_device(id)
    // self.devices.get(id)
  }

  pub fn get_device_mut(&mut self, id: &PeerId) -> Option<&mut (Device, Option<bool>)> {
    self.store.get_device_mut(id)
  }
  /// Add a device to the list of paired devices
  pub fn add_device(&mut self, device: Device) {
    self.store.save_device(device.peer, device, None);
  }

  /// Remove a paired  device
  pub fn remove_device(&mut self, id: &PeerId) -> Option<(Device, Option<bool>)> {
    self.store.remove_device(id)
  }

  pub fn device_exists(&self, id: &PeerId) -> bool {
    self.store.contains_device(id)
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
  /// Daemon
  daemon: SharedDaemon,
  /// Network Manager
  network_manager: NetworkManager,
  // Sender from sending frontend packets
  send_packet_rx: Option<mpsc::Receiver<Action>>,
  /// Context that is shared between all modules
  ctx: Arc<Mutex<MulticonnectCtx>>,
}

impl ModuleManager {
  /// Create a new module manager
  pub async fn new(network_manager: NetworkManager, daemon: SharedDaemon) -> Self {
    let (send_packet_tx, send_packet_rx) = mpsc::channel(100);
    Self {
      ctx: Arc::new(Mutex::new(
        MulticonnectCtx::new(send_packet_tx, Device::this(network_manager.get_local_peer_id())).await,
      )),
      modules: Vec::new(),
      daemon,
      network_manager,
      send_packet_rx: Some(send_packet_rx),
    }
  }

  /// Register a module
  pub fn register<M: MulticonnectModule + 'static>(&mut self, module: M) {
    let boxed = Box::new(module);
    self.modules.push(boxed);
  }

  /// Calls the `on_frontend_packet` function of every register module

  pub async fn start(&'static mut self) -> Result<(), Box<dyn std::error::Error>> {
    let ctx = self.ctx.clone();
    let mut periodic_interval = time::interval(Duration::from_millis(20));

    let daemon_clone = self.daemon.clone();
    let mut recv_frontend_packet_rx = self.daemon.recv_packet_channel();
    let mut recv_peer_packet_rx = self.network_manager.recv_event_channel();
    let send_frontend_packet_tx = self.daemon.send_packet_channel();
    let send_network_command = self.network_manager.send_command_channel();

    for module in self.modules.iter_mut() {
      module.init(ctx.clone()).await;
    }

    let mut send_packet_rx = self.send_packet_rx.take().unwrap();
    let mut modules = std::mem::take(&mut self.modules);
    tokio::spawn(async move {
      loop {
        tokio::select! {
          event = recv_frontend_packet_rx.recv() => if let Ok(event) = event {
            debug!("Calling on_frontend_event");
            let mut ctx = ctx.lock().await;
            for module in modules.iter_mut() {
              module.on_frontend_event(event.clone(), &mut ctx).await;
            }
          },

          event = recv_peer_packet_rx.recv() => if let Ok(event) = event {
            debug!("Calling on_network_event");
            let mut ctx = ctx.lock().await;
            for module in modules.iter_mut() {
              module.on_network_event(event.clone(), &mut ctx).await;
            }
          } else {
            error!("Error reciving peer packet channel");
          },

          action = send_packet_rx.recv() => if let Some(action) = action {

            match action {
                Action::SendFrontend(packet)=> { let _ = send_frontend_packet_tx.send(packet.to_owned()).await; },
                Action::SendPeer(peer_id,packet)=>{ let _ = send_network_command.send(NetworkCommand::SendPacket(peer_id,packet)).await; },
                Action::ApproveStream(peer_id) => { let _ = send_network_command.send(NetworkCommand::ApproveStream(peer_id)).await; },
                Action::DenyStream(peer_id) => { let _ = send_network_command.send(NetworkCommand::DenyStream(peer_id)).await; },
                Action::OpenStream(peer_id) => { let _ = send_network_command.send(NetworkCommand::OpenStream(peer_id)).await; },
                Action::CloseStream(peer_id) => { let _ = send_network_command.send(NetworkCommand::CloseStream(peer_id)); },
            };
          },
          _ = periodic_interval.tick() => {
            let mut ctx = ctx.lock().await;
            for module in modules.iter_mut() {
              module.periodic(&mut ctx).await;
            }
          }
        }
      }
    });

    self.network_manager.start().await?;
    daemon_clone.start().await;
    Ok(())
  }
}
