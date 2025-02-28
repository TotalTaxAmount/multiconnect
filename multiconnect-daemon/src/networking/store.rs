use std::{
  collections::HashMap,
  fs::{File, OpenOptions},
  io::{Read, Write},
};

use libp2p::PeerId;
use multiconnect_config::CONFIG;
use multiconnect_protocol::Peer;

pub struct Store {
  peers: HashMap<PeerId, Peer>,
}

impl Store {
  pub fn new() -> Self {
    Self { peers: Self::load() }
  }

  fn load() -> HashMap<PeerId, Peer> {
    let file = CONFIG.get_config_dir().join("saved");
    if !file.exists() {
      return HashMap::new();
    }

    let mut file = File::open(file).unwrap();
    let mut buf = Vec::new();
    file.read_to_end(&mut buf).unwrap();

    let peers: HashMap<PeerId, Peer> = bincode::deserialize(&buf).unwrap();
    // let mut key_map = HashMap::new();

    // for (peer_id_string, key_bytes) in keys {
    //   let peer_id = PeerId::from_str(&peer_id_string).unwrap();
    //   // let keypair = Keypair::from_protobuf_encoding(&key_bytes).unwrap();
    //   key_map.insert(peer_id, keypair);
    // }

    peers
  }

  fn save(&self) {
    let file = CONFIG.get_config_dir().join("saved");
    let mut file = OpenOptions::new().create(true).write(true).truncate(true).open(file).unwrap();

    let serialized = bincode::serialize(&self.peers).unwrap();
    file.write_all(&serialized).unwrap();
  }

  pub fn store_peer(&mut self, peer_id: PeerId, peer: Peer) {
    self.peers.insert(peer_id, peer);
    self.save();
  }

  pub fn retrieve_peer(&self, peer_id: PeerId) -> Option<&Peer> {
    self.peers.get(&peer_id)
  }

  pub fn remove_pper(&mut self, peer_id: PeerId) -> Option<Peer> {
    let peer = self.peers.remove(&peer_id);
    self.save();
    peer
  }
}
