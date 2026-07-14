use backend::core::bitfield::Bitfield;
use backend::core::types::InfoHash;
use backend::peer::connection::{PeerContext, PeerState};
use backend::core::types::PeerId;

fn make_info_hash(v: u8) -> InfoHash {
    InfoHash::from_bytes([v; 20])
}

#[test]
fn test_peer_context_new_defaults() {
    let ctx = PeerContext::new(PeerId::new_random(), make_info_hash(0xAA), 42);
    assert_eq!(ctx.state, PeerState::Connected);
    assert!(ctx.am_choking);
    assert!(ctx.peer_choking);
    assert!(!ctx.am_interested);
    assert!(!ctx.peer_interested);
    assert_eq!(ctx.peer_bitfield.total_pieces(), 42);
    assert_eq!(ctx.peer_bitfield.count_complete(), 0);
}

#[test]
fn test_update_interest_not_interested_when_same() {
    let our = Bitfield::new(10);
    let peer = Bitfield::new(10);
    let mut ctx = PeerContext::new(PeerId::new_random(), make_info_hash(0x01), 10);
    ctx.peer_bitfield = peer;
    ctx.update_interest(&our);
    assert!(!ctx.am_interested);
}

#[test]
fn test_update_interest_interested_when_peer_has_more() {
    let our = Bitfield::new(10);
    let mut peer = Bitfield::new(10);
    peer.set(0);
    peer.set(3);
    peer.set(7);
    let mut ctx = PeerContext::new(PeerId::new_random(), make_info_hash(0x02), 10);
    ctx.peer_bitfield = peer;
    ctx.update_interest(&our);
    assert!(ctx.am_interested);
}

#[test]
fn test_update_interest_not_interested_when_we_have_all() {
    let mut our = Bitfield::new(10);
    for i in 0..10 {
        our.set(i);
    }
    let mut peer = Bitfield::new(10);
    peer.set(0);
    peer.set(5);
    let mut ctx = PeerContext::new(PeerId::new_random(), make_info_hash(0x03), 10);
    ctx.peer_bitfield = peer;
    ctx.update_interest(&our);
    assert!(!ctx.am_interested);
}

#[test]
fn test_update_interest_interested_when_missing_one() {
    let mut our = Bitfield::new(10);
    for i in 0..9 {
        our.set(i);
    }
    let mut peer = Bitfield::new(10);
    for i in 0..10 {
        peer.set(i);
    }
    let mut ctx = PeerContext::new(PeerId::new_random(), make_info_hash(0x04), 10);
    ctx.peer_bitfield = peer;
    ctx.update_interest(&our);
    assert!(ctx.am_interested);
}

#[test]
fn test_peer_state_debug() {
    assert_eq!(format!("{:?}", PeerState::Handshaking), "Handshaking");
    assert_eq!(format!("{:?}", PeerState::Connected), "Connected");
    assert_eq!(format!("{:?}", PeerState::Disconnected), "Disconnected");
}

#[test]
fn test_peer_state_clone_eq() {
    let s = PeerState::Handshaking;
    assert_eq!(s.clone(), s);
    assert_ne!(s, PeerState::Connected);
}

#[test]
fn test_update_interest_on_empty_bitfields() {
    let our = Bitfield::new(0);
    let peer = Bitfield::new(0);
    let mut ctx = PeerContext::new(PeerId::new_random(), make_info_hash(0x05), 0);
    ctx.peer_bitfield = peer;
    ctx.update_interest(&our);
    assert!(!ctx.am_interested);
}

#[test]
fn test_update_interest_large_bitfield() {
    let our = Bitfield::new(1000);
    let mut peer = Bitfield::new(1000);
    peer.set(999);
    let mut ctx = PeerContext::new(PeerId::new_random(), make_info_hash(0x06), 1000);
    ctx.peer_bitfield = peer;
    ctx.update_interest(&our);
    assert!(ctx.am_interested);
}

#[test]
fn test_multiple_updates_flip_interest() {
    let our = Bitfield::new(5);
    let mut peer = Bitfield::new(5);
    let mut ctx = PeerContext::new(PeerId::new_random(), make_info_hash(0x07), 5);
    ctx.peer_bitfield = peer.clone();
    ctx.update_interest(&our);
    assert!(!ctx.am_interested);
    peer.set(2);
    ctx.peer_bitfield = peer;
    ctx.update_interest(&our);
    assert!(ctx.am_interested);
    ctx.update_interest(&our);
    assert!(ctx.am_interested);
}

#[test]
fn test_zero_pieces_context() {
    let ctx = PeerContext::new(PeerId::new_random(), make_info_hash(0x08), 0);
    assert_eq!(ctx.peer_bitfield.total_pieces(), 0);
    assert_eq!(ctx.peer_bitfield.count_complete(), 0);
}
