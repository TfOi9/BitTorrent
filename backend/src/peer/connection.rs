use crate::core::bitfield::Bitfield;
use crate::core::types::{InfoHash, PeerId};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PeerState {
    Handshaking,
    Connected,
    Disconnected,
}

#[derive(Debug)]
pub struct PeerContext {
    pub peer_id: PeerId,
    pub info_hash: InfoHash,
    pub peer_bitfield: Bitfield,
    pub state: PeerState,
    pub am_choking: bool,
    pub peer_choking: bool,
    pub am_interested: bool,
    pub peer_interested: bool,
}

impl PeerContext {
    pub fn new(peer_id: PeerId, info_hash: InfoHash, total_pieces: usize) -> Self {
        Self {
            peer_id,
            info_hash,
            peer_bitfield: Bitfield::new(total_pieces),
            state: PeerState::Connected,
            am_choking: true,
            peer_choking: true,
            am_interested: false,
            peer_interested: false,
        }
    }

    pub fn update_interest(&mut self, our_bitfield: &Bitfield) {
        self.am_interested = self
            .peer_bitfield
            .complete_pieces()
            .any(|i| !our_bitfield.has(i));
    }
}
