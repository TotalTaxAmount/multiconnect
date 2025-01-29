use libp2p::{Multiaddr, PeerId};

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq, Hash)]
pub struct Peer {
  pub peer_id: PeerId,
  pub multiaddr: Multiaddr,
}
