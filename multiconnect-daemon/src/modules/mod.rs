pub mod debug;
pub mod file_transfer;
pub mod pairing;
pub mod store;

use async_trait::async_trait;
use libp2p::PeerId;
use log::{debug, error, trace, warn};
use multiconnect_core::{Device, Packet, SavedDevice};
use std::{collections::HashMap, error::Error, sync::Arc};
use store::Store;
use tokio::sync::{mpsc, Mutex};

use crate::{
  networking::{NetworkCommand, NetworkEvent, NetworkManager},
  FrontendEvent, SharedDaemon,
};

#[derive(Debug, PartialEq)]
pub enum Action {
  SendFrontend(Packet),
  SendPeer(PeerId, Packet),
  OpenStream(PeerId),
  CloseStream(PeerId),
  // Update whitelist status for a peer
  UpdateWhitelist(PeerId, bool),
}

/// A module that can be used for features
#[async_trait]
pub trait MulticonnectModule: Send + Sync {
  /// Runs when the daemon receives a packet from the frontend
  async fn on_frontend_event(&mut self, event: FrontendEvent, ctx: &mut MulticonnectCtx) -> Result<(), Box<dyn Error>>;
  /// Runs when a peer is discovered
  async fn on_network_event(&mut self, event: NetworkEvent, ctx: &mut MulticonnectCtx) -> Result<(), Box<dyn Error>>;
  /// Runs once when the module is started
  async fn init(&mut self, ctx: Arc<Mutex<MulticonnectCtx>>) -> Result<(), Box<dyn Error>>;
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
  /// A list of devices: Saved device,
  devices: HashMap<PeerId, (SavedDevice, bool, bool)>,
}

impl MulticonnectCtx {
  /// Create a new instance of `MulticonnectCtx` <br />
  pub async fn new(send_packet_tx: mpsc::Sender<Action>, this_device: Device) -> Self {
    let store = Store::new().await.unwrap();
    let mut map = HashMap::new();
    let saved_devices = store.get_saved_devices();

    for (id, device) in saved_devices {
      map.insert(id.clone(), (device.clone(), false, false));
    }
    Self { action_tx: send_packet_tx, this_device, store, devices: map }
  }

  /// Send a packet to the frontend
  pub async fn send_to_frontend(&self, packet: Packet) {
    let _ = self.action_tx.send(Action::SendFrontend(packet)).await;
  }

  /// Send a packet to a peer
  pub async fn send_to_peer(&self, target: &PeerId, packet: Packet) {
    let _ = self.action_tx.send(Action::SendPeer(*target, packet)).await;
  }

  pub async fn open_stream(&self, peer_id: PeerId) {
    let _ = self.action_tx.send(Action::OpenStream(peer_id)).await;
  }

  pub async fn close_stream(&self, peer_id: PeerId) {
    let _ = self.action_tx.send(Action::CloseStream(peer_id)).await;
  }

  pub async fn save_store(&mut self) {
    for (peer_id, (device, _, _)) in &self.devices {
      if let Some(saved) = self.store.get_device_mut(peer_id) {
        *saved = device.clone();
      } else {
        self.store.save_device(*peer_id, device.clone());
      }
    }

    let all_device_ids: Vec<PeerId> = self.store.get_saved_devices().keys().cloned().collect();
    for peer_id in all_device_ids {
      if !self.devices.contains_key(&peer_id) {
        self.store.remove_device(&peer_id);
      }
    }

    self.store.save().await;
  }

  /// Get the HashMap of paired devices
  pub fn get_devices(&self) -> &HashMap<PeerId, (SavedDevice, bool, bool)> {
    &self.devices
  }

  pub async fn update_whitelist(&self, peer_id: PeerId, is_whitelisted: bool) {
    let _ = self.action_tx.send(Action::UpdateWhitelist(peer_id, is_whitelisted)).await;
  }

  /// Get a paired device
  pub fn get_device(&self, id: &PeerId) -> Option<&(SavedDevice, bool, bool)> {
    self.devices.get(id)
  }

  pub fn is_paired(&self, id: &PeerId) -> Option<bool> {
    Some(self.devices.get(id)?.0.is_paired())
  }

  /// Add a device to the list of paired devices
  pub fn add_device(&mut self, device: SavedDevice) {
    self.devices.insert(device.get_device().peer, (device, true, false));
  }

  pub fn get_device_mut(&mut self, id: &PeerId) -> Option<&mut (SavedDevice, bool, bool)> {
    if let Some(d) = self.devices.get_mut(id) {
      return Some(d);
    }
    None
  }

  /// Remove a paired  device
  pub fn remove_device(&mut self, id: &PeerId) -> Option<(SavedDevice, bool, bool)> {
    self.devices.remove(id)
  }

  pub fn device_exists(&self, id: &PeerId) -> bool {
    self.devices.contains_key(id)
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
  action_rx: Option<mpsc::Receiver<Action>>,
  /// Context that is shared between all modules
  ctx: Arc<Mutex<MulticonnectCtx>>,
}

impl ModuleManager {
  /// Create a new module manager
  pub async fn new(network_manager: NetworkManager, daemon: SharedDaemon) -> Self {
    let (action_tx, action_rx) = mpsc::channel(10_000);
    Self {
      ctx: Arc::new(Mutex::new(
        MulticonnectCtx::new(action_tx, Device::this(network_manager.get_local_peer_id())).await,
      )),
      modules: Vec::new(),
      daemon,
      network_manager,
      action_rx: Some(action_rx),
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

    let daemon_clone = self.daemon.clone();
    let mut recv_frontend_packet_rx = self.daemon.recv_packet_channel();
    let mut recv_peer_packet_rx = self.network_manager.recv_event_channel();
    let send_frontend_packet_tx = self.daemon.send_packet_channel();
    let send_network_command = self.network_manager.send_command_channel();

    for module in self.modules.iter_mut() {
      if let Err(e) = module.init(ctx.clone()).await {
        warn!("Error initlaizing module: {}", e);
      }
    }

    let mut action_rx = self.action_rx.take().unwrap();
    let mut modules = std::mem::take(&mut self.modules);
    tokio::spawn(async move {
      // let mut  = ctx.lock().await;

      loop {
        tokio::select! {
          event = recv_frontend_packet_rx.recv() => if let Ok(event) = event {
            trace!("Calling on_frontend_event");
            let mut ctx = ctx.lock().await;
            for module in modules.iter_mut() {
              if let Err(e) = module.on_frontend_event(event.clone(), &mut ctx).await {
                warn!("Error while running modules (on_frontend_event): {}", e);
              }
            }

            trace!("Done calling on_frontend_event");
          },

          event = recv_peer_packet_rx.recv() => match event {
            Ok(event) => {
              trace!("Calling on_network_event");
              let mut ctx = ctx.lock().await;
              for module in modules.iter_mut() {
                if let Err(e) = module.on_network_event(event.clone(), &mut ctx).await {
                  warn!("Error while running modules (on_network_event): {}", e);
                }
              }
              trace!("Done calling on_network_event");
            }
            Err(e) => {
              trace!("Error receiving peer packet channel {}", e);
            }
          },

          action = action_rx.recv() => if let Some(action) = action {
            match action {
                Action::SendFrontend(packet) => { let _ = send_frontend_packet_tx.send(packet.to_owned()).await; },
                Action::SendPeer(peer_id, packet) =>{ let _ = send_network_command.send(NetworkCommand::SendPacket(peer_id, packet)).await; },
                Action::OpenStream(peer_id) => { let _ = send_network_command.send(NetworkCommand::OpenStream(peer_id)).await; },
                Action::CloseStream(peer_id) => { let _ = send_network_command.send(NetworkCommand::CloseStream(peer_id)).await; },
                Action::UpdateWhitelist(peer_id, is_whitelisted) => { let _ = send_network_command.send(NetworkCommand::UpdateWhitelist(peer_id, is_whitelisted)).await; }
            };
          },
        }
      }
    });

    self.network_manager.start().await?;
    daemon_clone.start().await;
    Ok(())
  }
}
