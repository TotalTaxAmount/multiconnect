use libp2p::{Multiaddr, PeerId};

#[derive(Debug, Clone, serde::Serialize)]
pub struct Peer {
  pub peer_id: PeerId,
  pub multiaddr: Multiaddr,
}
