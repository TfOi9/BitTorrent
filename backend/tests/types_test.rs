use backend::core::types::*;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

#[test]
fn test_infohash_from_hex() {
    let hash = InfoHash::from_hex("abcdef0123456789abcdef0123456789abcdef01").unwrap();
    assert_eq!(hash.as_bytes()[0], 0xab);
    assert_eq!(hash.as_bytes()[19], 0x01);
}

#[test]
fn test_infohash_from_hex_invalid_length() {
    assert!(InfoHash::from_hex("too_short").is_err());
}

#[test]
fn test_infohash_from_hex_all_zeros() {
    let hash = InfoHash::from_hex("0000000000000000000000000000000000000000").unwrap();
    for byte in hash.as_bytes() {
        assert_eq!(*byte, 0);
    }
}

#[test]
fn test_infohash_from_hex_all_ff() {
    let hash = InfoHash::from_hex("ffffffffffffffffffffffffffffffffffffffff").unwrap();
    for byte in hash.as_bytes() {
        assert_eq!(*byte, 0xFF);
    }
}

#[test]
fn test_infohash_from_hex_invalid_hex_chars() {
    assert!(InfoHash::from_hex("gggggggggggggggggggggggggggggggggggggggg").is_err());
}

#[test]
fn test_infohash_from_hex_empty() {
    assert!(InfoHash::from_hex("").is_err());
}

#[test]
fn test_infohash_from_hex_wrong_length_short() {
    assert!(InfoHash::from_hex("abcdef").is_err());
}

#[test]
fn test_infohash_from_hex_wrong_length_long() {
    let long = "a".repeat(42);
    assert!(InfoHash::from_hex(&long).is_err());
}

#[test]
fn test_infohash_equality() {
    let h1 = InfoHash::from_hex("abcdef0123456789abcdef0123456789abcdef01").unwrap();
    let h2 = InfoHash::from_hex("abcdef0123456789abcdef0123456789abcdef01").unwrap();
    let h3 = InfoHash::from_hex("0000000000000000000000000000000000000000").unwrap();
    assert_eq!(h1, h2);
    assert_ne!(h1, h3);
}

#[test]
fn test_infohash_display() {
    let hash = InfoHash::from_hex("abcdef0123456789abcdef0123456789abcdef01").unwrap();
    let s = format!("{}", hash);
    assert_eq!(s, "abcdef0123456789abcdef0123456789abcdef01");
}

#[test]
fn test_infohash_debug() {
    let hash = InfoHash::from_hex("abcdef0123456789abcdef0123456789abcdef01").unwrap();
    let s = format!("{:?}", hash);
    assert!(s.starts_with("InfoHash("));
}

#[test]
fn test_infohash_short_hex() {
    let hash = InfoHash::from_hex("abcdef0123456789abcdef0123456789abcdef01").unwrap();
    let s = hash.short_hex();
    assert_eq!(s, "abcd...");
}

#[test]
fn test_infohash_from_bytes() {
    let bytes = [0xde, 0xad, 0xbe, 0xef,
                 0x00, 0x01, 0x02, 0x03,
                 0x04, 0x05, 0x06, 0x07,
                 0x08, 0x09, 0x0a, 0x0b,
                 0x0c, 0x0d, 0x0e, 0x0f];
    let hash = InfoHash::from_bytes(bytes);
    assert_eq!(hash.as_bytes(), &bytes);
}

#[test]
fn test_infohash_from_trait() {
    let bytes = [1u8; SHA1_LEN];
    let hash: InfoHash = bytes.into();
    assert_eq!(hash.as_bytes(), &bytes);
}

#[test]
fn test_infohash_as_slice() {
    let hash = InfoHash::from_hex("00112233445566778899aabbccddeeff00112233").unwrap();
    assert_eq!(hash.as_slice(), hash.as_bytes().as_slice());
}

#[test]
fn test_peerid_new_random() {
    let id1 = PeerId::new_random();
    let id2 = PeerId::new_random();
    assert_eq!(&id1.0[..6], b"-RB000");
    assert_ne!(id1, id2);
}

#[test]
fn test_peerid_new_random_many() {
    let mut ids = Vec::new();
    for _ in 0..100 {
        ids.push(PeerId::new_random());
    }
    for i in 0..ids.len() {
        for j in (i + 1)..ids.len() {
            assert_ne!(ids[i], ids[j]);
        }
    }
}

#[test]
fn test_peerid_display() {
    let id = PeerId::new_random();
    let s = format!("{}", id);
    assert_eq!(&id.0[..8], b"-RB0001-");
    assert!(s.len() == 20 || s.len() == 40);
}

#[test]
fn test_peerid_debug() {
    let id = PeerId::new_random();
    let s = format!("{:?}", id);
    assert!(s.starts_with("PeerId("));
}

#[test]
fn test_peerid_as_slice() {
    let id = PeerId::new_random();
    assert_eq!(id.as_slice().len(), SHA1_LEN);
}

#[test]
fn test_compact_peer_v4() {
    let bytes = [192, 168, 1, 100, 0x1A, 0xE1];
    let peer = PeerAddr::from_compact_v4(&bytes).unwrap();
    assert_eq!(peer.ip, IpAddr::V4(Ipv4Addr::new(192, 168, 1, 100)));
    assert_eq!(peer.port, 6881);
}

#[test]
fn test_compact_peer_v4_zero_port() {
    let bytes = [127, 0, 0, 1, 0, 0];
    let peer = PeerAddr::from_compact_v4(&bytes).unwrap();
    assert_eq!(peer.ip, IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)));
    assert_eq!(peer.port, 0);
}

#[test]
fn test_compact_peer_v4_boundaries() {
    let bytes = [0, 0, 0, 0, 0, 0];
    let peer = PeerAddr::from_compact_v4(&bytes).unwrap();
    assert_eq!(peer.ip, IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0)));
    assert_eq!(peer.port, 0);

    let bytes = [255, 255, 255, 255, 255, 255];
    let peer = PeerAddr::from_compact_v4(&bytes).unwrap();
    assert_eq!(peer.ip, IpAddr::V4(Ipv4Addr::new(255, 255, 255, 255)));
    assert_eq!(peer.port, 65535);
}

#[test]
fn test_compact_peer_v6() {
    let bytes = [
        0x20, 0x01, 0x0d, 0xb8, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01,
        0x1F, 0x90,
    ];
    let peer = PeerAddr::from_compact_v6(&bytes).unwrap();
    assert_eq!(peer.ip, IpAddr::V6(Ipv6Addr::new(
        0x2001, 0x0db8, 0, 0, 0, 0, 0, 0x0001,
    )));
    assert_eq!(peer.port, 8080);
}

#[test]
fn test_compact_peer_v4_invalid_length() {
    assert!(PeerAddr::from_compact_v4(&[0; 4]).is_err());
    assert!(PeerAddr::from_compact_v4(&[0; 7]).is_err());
    assert!(PeerAddr::from_compact_v4(&[]).is_err());
}

#[test]
fn test_compact_peer_v6_invalid_length() {
    assert!(PeerAddr::from_compact_v6(&[0; 17]).is_err());
    assert!(PeerAddr::from_compact_v6(&[0; 19]).is_err());
    assert!(PeerAddr::from_compact_v6(&[]).is_err());
}

#[test]
fn test_peer_addr_new() {
    let peer = PeerAddr::new(IpAddr::V4(Ipv4Addr::new(10, 0, 0, 1)), 1337);
    assert_eq!(peer.ip, IpAddr::V4(Ipv4Addr::new(10, 0, 0, 1)));
    assert_eq!(peer.port, 1337);
}

#[test]
fn test_peer_addr_to_socket_string() {
    let peer = PeerAddr::new(IpAddr::V4(Ipv4Addr::new(10, 0, 0, 1)), 8080);
    assert_eq!(peer.to_socket_string(), "10.0.0.1:8080");
}

#[test]
fn test_peer_addr_display() {
    let peer = PeerAddr::new(IpAddr::V4(Ipv4Addr::new(1, 2, 3, 4)), 56789);
    assert_eq!(format!("{}", peer), "1.2.3.4:56789");
}

#[test]
fn test_piece_index_operations() {
    let p = PieceIndex(42);
    assert_eq!(p.as_usize(), 42);
    assert_eq!(format!("{}", p), "42");
    assert_eq!(format!("{:?}", p), "PieceIndex(42)");
}

#[test]
fn test_piece_index_cmp() {
    assert!(PieceIndex(1) < PieceIndex(2));
    assert!(PieceIndex(10) > PieceIndex(5));
    assert_eq!(PieceIndex(3), PieceIndex(3));
}

#[test]
fn test_piece_index_zero() {
    let p = PieceIndex(0);
    assert_eq!(p.as_usize(), 0);
}

#[test]
fn test_block_request_new() {
    let br = BlockRequest::new(
        PieceIndex(1),
        BlockOffset(1024),
        BlockLength(16384),
    );
    assert_eq!(br.index, PieceIndex(1));
    assert_eq!(br.begin, BlockOffset(1024));
    assert_eq!(br.length, BlockLength(16384));
}

#[test]
fn test_block_request_debug() {
    let br = BlockRequest::new(
        PieceIndex(0),
        BlockOffset(0),
        BlockLength(BLOCK_LEN),
    );
    let s = format!("{:?}", br);
    assert!(s.contains("0"));
}

#[test]
fn test_constants() {
    assert_eq!(SHA1_LEN, 20);
    assert_eq!(BLOCK_LEN, 16 * 1024);
    assert_eq!(PROTOCOL_STR, b"BitTorrent protocol");
    assert_eq!(RESERVED_LEN, 8);
}
