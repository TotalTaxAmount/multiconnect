use std::{
  collections::HashMap,
  fs::{remove_file, File, OpenOptions},
  io::{Read, Write},
};

use libp2p::PeerId;
use log::error;
use multiconnect_config::CONFIG;
use multiconnect_core::SavedDevice;
use thiserror::Error;

const FILENAME: &str = "saved_devices.json";

pub struct Store {
  devices: HashMap<PeerId, SavedDevice>,
}

#[derive(Debug, Error)]
pub enum StoreError {
  #[error("I/O error: {0}")]
  Io(#[from] tokio::io::Error),

  #[error("Serde error: {0}")]
  SerdeError(#[from] serde_json::Error),

  #[error("Failed to load devices: {0}")]
  LoadError(String),
}

impl Store {
  pub async fn new() -> Result<Self, StoreError> {
    Ok(Self { devices: Self::load().await? })
  }

  async fn load() -> Result<HashMap<PeerId, SavedDevice>, StoreError> {
    let cfg = CONFIG.get().unwrap();
    let path = cfg.read().await.get_config_dir().join(FILENAME);
    if !path.exists() {
      return Ok(HashMap::new());
    }

    let mut file = File::open(&path)?;
    let mut buf = Vec::new();
    file.read_to_end(&mut buf)?;

    let raw = String::from_utf8(buf).unwrap();

    if let Ok(saved_devices) = serde_json::from_str(&raw) {
      return Ok(saved_devices);
    } else {
      // error!("Failed to read saved saved_devices from file");
      let _ = remove_file(path);
      // return Ok(HashMap::new());
      return Err(StoreError::LoadError("Failed to read saved devices from file".to_string()));
    }
  }

  pub async fn save(&self) -> Result<(), StoreError> {
    let cfg = CONFIG.get().unwrap();
    let file = cfg.read().await.get_config_dir().join(FILENAME);
    let mut file = OpenOptions::new().create(true).write(true).truncate(true).open(file)?;

    let json = serde_json::to_string(&self.devices)?;
    file.write_all(&json.as_bytes())?;

    Ok(())
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
