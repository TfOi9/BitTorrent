// DHT integration test —— auto launch Go DHT Sidecar.
// To run: cargo test --test dht_integration_test -- --nocapture

mod common;

use backend::core::types::InfoHash;
use backend::dht::DhtClient;
use backend::core::types::PeerAddr;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

async fn connect_to_sidecar() -> DhtClient {
    let guard = common::ensure_sidecar();
    DhtClient::connect(&guard.endpoint)
        .await
        .expect("should connect to sidecar")
}

fn info_hash(b: u8) -> InfoHash {
    InfoHash::from_bytes([b; 20])
}

fn peer(port: u16) -> PeerAddr {
    PeerAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), port)
}

fn peer_v6(port: u16) -> PeerAddr {
    PeerAddr::new(IpAddr::V6(Ipv6Addr::new(0, 0, 0, 0, 0, 0, 0, 1)), port)
}

// ---------------------------------------------------------------------------
// 1. Connection & health check
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_dht_connect_and_health() {
    let mut client = connect_to_sidecar().await;
    assert!(client.health_check().await.unwrap());
}

#[tokio::test]
async fn test_multiple_health_checks() {
    let mut client = connect_to_sidecar().await;
    for _ in 0..10 {
        assert!(client.health_check().await.unwrap());
    }
}

#[tokio::test]
async fn test_connect_to_bad_endpoint() {
    let result = DhtClient::connect("http://127.0.0.1:1").await;
    assert!(result.is_err());
}

// ---------------------------------------------------------------------------
// 2. get_peers —— empty / not found
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_get_peers_empty() {
    let mut client = connect_to_sidecar().await;
    let peers = client.get_peers(&info_hash(0xAA)).await.unwrap();
    assert!(peers.is_empty());
}

#[tokio::test]
async fn test_get_peers_multiple_empty_hashes() {
    let mut client = connect_to_sidecar().await;
    for b in 0xC0..0xCAu8 {
        let peers = client.get_peers(&info_hash(b)).await.unwrap();
        assert!(peers.is_empty(), "hash {:02x}... should have no peers", b);
    }
}

// ---------------------------------------------------------------------------
// 3. announce_peer —— basic
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_announce_and_get_peers() {
    let mut client = connect_to_sidecar().await;

    let hash = info_hash(0xBB);
    let my_peer = peer(6881);
    let ok = client.announce_peer(&hash, &my_peer).await.unwrap();
    assert!(ok);

    tokio::time::sleep(std::time::Duration::from_millis(500)).await;

    let peers = client.get_peers(&hash).await.unwrap();
    assert!(!peers.is_empty());
    assert!(peers.iter().any(|p| p.port == 6881));
}

#[tokio::test]
async fn test_announce_peer_returns_true() {
    let mut client = connect_to_sidecar().await;
    let ok = client.announce_peer(&info_hash(0x01), &peer(7000)).await.unwrap();
    assert!(ok, "announce_peer should return true on success");
}

// ---------------------------------------------------------------------------
// 4. Multiple peers for the SAME info_hash
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_multiple_peers_same_hash() {
    let mut client = connect_to_sidecar().await;
    let hash = info_hash(0xCC);

    let ports: &[u16] = &[6881, 6882, 6883, 6884, 6885];
    for &port in ports {
        assert!(client.announce_peer(&hash, &peer(port)).await.unwrap());
    }

    tokio::time::sleep(std::time::Duration::from_millis(500)).await;

    let peers = client.get_peers(&hash).await.unwrap();
    assert_eq!(peers.len(), ports.len(), "should return all announced peers");
    for &port in ports {
        assert!(
            peers.iter().any(|p| p.port == port),
            "missing peer with port {port}"
        );
    }
}

#[tokio::test]
async fn test_large_number_of_peers_same_hash() {
    let mut client = connect_to_sidecar().await;
    let hash = info_hash(0xDD);
    let peer_count: u16 = 50;

    for i in 0..peer_count {
        let port = 7000 + i;
        assert!(client.announce_peer(&hash, &peer(port)).await.unwrap());
    }

    tokio::time::sleep(std::time::Duration::from_millis(800)).await;

    let peers = client.get_peers(&hash).await.unwrap();
    assert_eq!(peers.len(), peer_count as usize);
}

// ---------------------------------------------------------------------------
// 5. Multiple INDEPENDENT info_hashes (isolation)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_hash_isolation() {
    let mut client = connect_to_sidecar().await;

    let hash_a = info_hash(0x10);
    let hash_b = info_hash(0x20);

    client.announce_peer(&hash_a, &peer(8001)).await.unwrap();
    client.announce_peer(&hash_b, &peer(8002)).await.unwrap();

    tokio::time::sleep(std::time::Duration::from_millis(500)).await;

    let peers_a = client.get_peers(&hash_a).await.unwrap();
    let peers_b = client.get_peers(&hash_b).await.unwrap();

    assert_eq!(peers_a.len(), 1);
    assert_eq!(peers_a[0].port, 8001);
    assert_eq!(peers_b.len(), 1);
    assert_eq!(peers_b[0].port, 8002);
}

#[tokio::test]
async fn test_hash_isolation_with_overlap() {
    let mut client = connect_to_sidecar().await;

    let hash_a = info_hash(0x30);
    let hash_b = info_hash(0x31);

    let shared = peer(9000);
    client.announce_peer(&hash_a, &shared).await.unwrap();
    client.announce_peer(&hash_a, &peer(9001)).await.unwrap();
    client.announce_peer(&hash_b, &shared).await.unwrap();
    client.announce_peer(&hash_b, &peer(9002)).await.unwrap();

    tokio::time::sleep(std::time::Duration::from_millis(500)).await;

    let peers_a = client.get_peers(&hash_a).await.unwrap();
    let peers_b = client.get_peers(&hash_b).await.unwrap();

    assert_eq!(peers_a.len(), 2);
    assert_eq!(peers_b.len(), 2);

    assert!(peers_a.iter().any(|p| p.port == 9000));
    assert!(peers_b.iter().any(|p| p.port == 9000));
    assert!(peers_a.iter().any(|p| p.port == 9001));
    assert!(peers_b.iter().any(|p| p.port == 9002));
}

// ---------------------------------------------------------------------------
// 6. IPv6
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_announce_and_get_peer_ipv6() {
    let mut client = connect_to_sidecar().await;
    let hash = info_hash(0xE0);

    let v6_peer = peer_v6(6881);
    assert!(client.announce_peer(&hash, &v6_peer).await.unwrap());

    tokio::time::sleep(std::time::Duration::from_millis(500)).await;

    let peers = client.get_peers(&hash).await.unwrap();
    assert!(!peers.is_empty());
    assert!(peers.iter().any(|p| p.ip == v6_peer.ip && p.port == 6881));
}

#[tokio::test]
async fn test_mixed_ipv4_ipv6_peers() {
    let mut client = connect_to_sidecar().await;
    let hash = info_hash(0xE1);

    client.announce_peer(&hash, &peer(7001)).await.unwrap();
    client.announce_peer(&hash, &peer_v6(7002)).await.unwrap();

    tokio::time::sleep(std::time::Duration::from_millis(500)).await;

    let peers = client.get_peers(&hash).await.unwrap();
    assert_eq!(peers.len(), 2);
    let has_v4 = peers.iter().any(|p| matches!(p.ip, IpAddr::V4(_)));
    let has_v6 = peers.iter().any(|p| matches!(p.ip, IpAddr::V6(_)));
    assert!(has_v4);
    assert!(has_v6);
}

// ---------------------------------------------------------------------------
// 7. Edge cases: port 0, port 65535, duplicate announces
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_announce_port_zero_rejected() {
    let mut client = connect_to_sidecar().await;
    let hash = info_hash(0xF0);

    let _ = client.announce_peer(&hash, &peer(0)).await;

    tokio::time::sleep(std::time::Duration::from_millis(300)).await;

    let peers = client.get_peers(&hash).await.unwrap();
    assert!(
        peers.iter().all(|p| p.port != 0),
        "peers with port=0 should be filtered out"
    );
}

#[tokio::test]
async fn test_announce_port_max_boundary() {
    let mut client = connect_to_sidecar().await;
    let hash = info_hash(0xF1);
    let max_port_peer = peer(65535);

    assert!(client.announce_peer(&hash, &max_port_peer).await.unwrap());

    tokio::time::sleep(std::time::Duration::from_millis(500)).await;

    let peers = client.get_peers(&hash).await.unwrap();
    assert!(!peers.is_empty());
    assert!(peers.iter().any(|p| p.port == 65535));
}

#[tokio::test]
async fn test_duplicate_announce_idempotent() {
    let mut client = connect_to_sidecar().await;
    let hash = info_hash(0xF2);
    let p = peer(7777);

    for _ in 0..3 {
        assert!(client.announce_peer(&hash, &p).await.unwrap());
    }

    tokio::time::sleep(std::time::Duration::from_millis(500)).await;

    let peers = client.get_peers(&hash).await.unwrap();
    let count = peers.iter().filter(|x| x.port == 7777).count();
    assert!(count >= 1);
}

// ---------------------------------------------------------------------------
// 8. Sequential consistency: announce / get sequences
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_sequential_announce_get_sequence() {
    let mut client = connect_to_sidecar().await;
    let hash = info_hash(0x50);

    for round in 1..=5u16 {
        let port = 10000 + round;
        assert!(client.announce_peer(&hash, &peer(port)).await.unwrap());
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;

        let peers = client.get_peers(&hash).await.unwrap();
        assert!(
            peers.iter().any(|p| p.port == port),
            "round {round}: should find just-announced peer"
        );
        for prev in 1..round {
            assert!(
                peers.iter().any(|p| p.port == 10000 + prev),
                "round {round}: should still find peer from round {prev}"
            );
        }
    }
}

#[tokio::test]
async fn test_get_peers_immediately_after_announce() {
    let mut client = connect_to_sidecar().await;
    let hash = info_hash(0x51);
    let p = peer(11000);

    client.announce_peer(&hash, &p).await.unwrap();
    let peers = client.get_peers(&hash).await.unwrap();
    let _ = peers.len();
}

// ---------------------------------------------------------------------------
// 9. Stress / concurrency via sequential bulk operations
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_bulk_announce_then_get() {
    let mut client = connect_to_sidecar().await;
    let hash = info_hash(0x60);
    let count: u16 = 100;

    for i in 0..count {
        assert!(client.announce_peer(&hash, &peer(12000 + i)).await.unwrap());
    }

    tokio::time::sleep(std::time::Duration::from_millis(1500)).await;

    let peers = client.get_peers(&hash).await.unwrap();
    assert_eq!(peers.len(), count as usize);
}

#[tokio::test]
async fn test_get_peers_repeat_stress() {
    let mut client = connect_to_sidecar().await;
    let hash = info_hash(0x61);

    for port in [5001, 5002, 5003] {
        client.announce_peer(&hash, &peer(port)).await.unwrap();
    }
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;

    for _ in 0..20 {
        let peers = client.get_peers(&hash).await.unwrap();
        assert_eq!(peers.len(), 3);
    }
}

// ---------------------------------------------------------------------------
// 10. Interleaved operations
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_interleaved_announce_and_get() {
    let mut client = connect_to_sidecar().await;

    let hash_a = info_hash(0x70);
    let hash_b = info_hash(0x71);

    client.announce_peer(&hash_a, &peer(13001)).await.unwrap();
    let _pa = client.get_peers(&hash_a).await.unwrap();
    
    client.announce_peer(&hash_b, &peer(13002)).await.unwrap();
    let _pb = client.get_peers(&hash_b).await.unwrap();

    client.announce_peer(&hash_a, &peer(13003)).await.unwrap();
    let _pa2 = client.get_peers(&hash_a).await.unwrap();

    tokio::time::sleep(std::time::Duration::from_millis(500)).await;

    let final_a = client.get_peers(&hash_a).await.unwrap();
    let final_b = client.get_peers(&hash_b).await.unwrap();

    assert!(final_a.iter().any(|p| p.port == 13001));
    assert!(final_a.iter().any(|p| p.port == 13003));
    assert!(final_b.iter().any(|p| p.port == 13002));

    assert!(!final_a.iter().any(|p| p.port == 13002));
    assert!(!final_b.iter().any(|p| p.port == 13001));
}

// ---------------------------------------------------------------------------
// 11. Multiple clients (same sidecar)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_multiple_clients_same_sidecar() {
    let guard = common::ensure_sidecar();

    let mut client1 = DhtClient::connect(&guard.endpoint).await.unwrap();
    let mut client2 = DhtClient::connect(&guard.endpoint).await.unwrap();

    let hash = info_hash(0x80);

    // client1 announces
    client1.announce_peer(&hash, &peer(14001)).await.unwrap();
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;

    // client2 should see the same peer
    let peers = client2.get_peers(&hash).await.unwrap();
    assert!(!peers.is_empty());
    assert!(peers.iter().any(|p| p.port == 14001));
}

// ---------------------------------------------------------------------------
// 12. Different IPs (non-loopback simulation)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_peers_with_different_ips() {
    let mut client = connect_to_sidecar().await;
    let hash = info_hash(0x90);

    let peer1 = PeerAddr::new(IpAddr::V4(Ipv4Addr::new(192, 168, 1, 1)), 6881);
    let peer2 = PeerAddr::new(IpAddr::V4(Ipv4Addr::new(10, 0, 0, 1)), 6882);
    let peer3 = PeerAddr::new(IpAddr::V4(Ipv4Addr::new(172, 16, 0, 1)), 6883);

    client.announce_peer(&hash, &peer1).await.unwrap();
    client.announce_peer(&hash, &peer2).await.unwrap();
    client.announce_peer(&hash, &peer3).await.unwrap();

    tokio::time::sleep(std::time::Duration::from_millis(500)).await;

    let peers = client.get_peers(&hash).await.unwrap();
    assert_eq!(peers.len(), 3);

    let ips: Vec<IpAddr> = peers.iter().map(|p| p.ip).collect();
    assert!(ips.contains(&peer1.ip));
    assert!(ips.contains(&peer2.ip));
    assert!(ips.contains(&peer3.ip));
}

// ---------------------------------------------------------------------------
// 13. PeerAddr complete structure check
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_peer_addr_round_trip() {
    let mut client = connect_to_sidecar().await;
    let hash = info_hash(0xA0);

    let original = PeerAddr::new(
        IpAddr::V4(Ipv4Addr::new(198, 51, 100, 42)),
        9999,
    );

    client.announce_peer(&hash, &original).await.unwrap();
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;

    let peers = client.get_peers(&hash).await.unwrap();
    assert_eq!(peers.len(), 1);
    assert_eq!(peers[0].ip, original.ip);
    assert_eq!(peers[0].port, original.port);
}

// ===========================================================================
// Invalid input
// ===========================================================================

#[tokio::test]
async fn test_connect_empty_endpoint() {
    let result = DhtClient::connect("").await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_connect_invalid_url() {
    let result = DhtClient::connect("not-a-valid-url!!!").await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_get_peers_zero_hash() {
    let mut client = connect_to_sidecar().await;
    let peers = client.get_peers(&info_hash(0x00)).await.unwrap();
    let _ = peers.len();
}

#[tokio::test]
async fn test_get_peers_all_same_byte_hash() {
    let mut client = connect_to_sidecar().await;
    for b in [0x00u8, 0xFF, 0x55, 0xAA] {
        let peers = client.get_peers(&info_hash(b)).await.unwrap();
        let _ = peers.len();
    }
}