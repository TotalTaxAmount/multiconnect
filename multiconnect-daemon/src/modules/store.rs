use std::{
  collections::HashMap,
  fs::{remove_file, File, OpenOptions},
  io::{Read, Write},
};

use libp2p::PeerId;
use log::error;
use multiconnect_config::CONFIG;
use multiconnect_core::SavedDevice;

const FILENAME: &str = "saved_devices.json";

pub struct Store {
  devices: HashMap<PeerId, SavedDevice>,
}

impl Store {
  pub async fn new() -> Self {
    Self { devices: Self::load().await }
  }

  async fn load() -> HashMap<PeerId, SavedDevice> {
    let cfg = CONFIG.get().unwrap();
    let path = cfg.read().await.get_config_dir().join(FILENAME);
    if !path.exists() {
      return HashMap::new();
    }

    let mut file = File::open(&path).unwrap();
    let mut buf = Vec::new();
    file.read_to_end(&mut buf).unwrap();

    let raw = String::from_utf8(buf).unwrap();

    if let Ok(saved_devices) = serde_json::from_str(&raw) {
      return saved_devices;
    } else {
      error!("Failed to read saved saved_devices from file");
      let _ = remove_file(path);
      return HashMap::new();
    }
  }

  pub async fn save(&self) {
    let cfg = CONFIG.get().unwrap();
    let file = cfg.read().await.get_config_dir().join(FILENAME);
    let mut file = OpenOptions::new().create(true).write(true).truncate(true).open(file).unwrap();

    let json = serde_json::to_string(&self.devices).unwrap();
    file.write_all(&json.as_bytes()).unwrap();
  }

  pub fn save_device(&mut self, peer_id: PeerId, device: SavedDevice) {
    self.devices.insert(peer_id, device);
  }

  pub fn get_saved_devices(&self) -> &HashMap<PeerId, SavedDevice> {
    &self.devices
  }

  pub fn check_paired(&self, peer_id: PeerId) -> Option<&SavedDevice> {
    self.devices.get(&peer_id)
  }

  pub fn get_device(&self, peer_id: &PeerId) -> Option<&SavedDevice> {
    self.devices.get(peer_id)
  }

  pub fn get_device_mut(&mut self, peer_id: &PeerId) -> Option<&mut SavedDevice> {
    self.devices.get_mut(peer_id)
  }

  pub fn remove_device(&mut self, peer_id: &PeerId) -> Option<SavedDevice> {
    self.devices.remove(peer_id)
  }

  pub fn contains_device(&self, peer_id: &PeerId) -> bool {
    self.devices.contains_key(peer_id)
  }

  pub fn set_last_seen(&mut self, peer_id: &PeerId, last_seen: u64) {
    if let Some(d) = self.get_device_mut(peer_id) {
      d.set_last_seen(last_seen);
    }
  }

  pub fn set_paired(&mut self, peer_id: &PeerId, paired: bool) {
    if let Some(d) = self.get_device_mut(peer_id) {
      d.set_paired(paired);
    }
  }
}
