use libp2p::{Multiaddr, PeerId};

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Peer {
  pub peer_id: PeerId,
  pub multiaddr: Multiaddr,
}
