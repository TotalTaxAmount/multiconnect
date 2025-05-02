use std::{
  collections::HashMap,
  fs::{remove_file, File, OpenOptions},
  io::{Read, Write},
};

use libp2p::PeerId;
use log::error;
use multiconnect_config::CONFIG;
use multiconnect_protocol::Peer;

pub struct Store {
  peers: HashMap<PeerId, Peer>,
}

impl Store {
  pub async fn new() -> Self {
    Self { peers: Self::load().await }
  }

  async fn load() -> HashMap<PeerId, Peer> {
    let path = CONFIG.read().await.get_config_dir().join("saved");
    if !path.exists() {
      return HashMap::new();
    }

    let mut file = File::open(&path).unwrap();
    let mut buf = Vec::new();
    file.read_to_end(&mut buf).unwrap();

    if let Ok(peers) = bincode::deserialize::<HashMap<PeerId, Peer>>(&buf) {
      return peers;
    } else {
      error!("Failed to read saved peers from file");
      let _ = remove_file(path);
      return HashMap::new();
    }
  }

  async fn save(&self) {
    let file = CONFIG.read().await.get_config_dir().join("saved");
    let mut file = OpenOptions::new().create(true).write(true).truncate(true).open(file).unwrap();

    let serialized = bincode::serialize(&self.peers).unwrap();
    file.write_all(&serialized).unwrap();
  }

  pub async fn store_peer(&mut self, peer_id: PeerId, peer: Peer) {
    self.peers.insert(peer_id, peer);
    self.save().await;
  }

  pub fn retrieve_peer(&self, peer_id: PeerId) -> Option<&Peer> {
    self.peers.get(&peer_id)
  }

  pub async fn remove_peer(&mut self, peer_id: PeerId) -> Option<Peer> {
    let peer = self.peers.remove(&peer_id);
    self.save().await;
    peer
  }
}
