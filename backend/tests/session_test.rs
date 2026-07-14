use std::net::{IpAddr, Ipv4Addr};

use sha1::Digest;
use tokio::io::AsyncReadExt;
use tokio::io::AsyncWriteExt;
use tokio::net::TcpListener;

use backend::core::bitfield::Bitfield;
use backend::core::types::InfoHash;
use backend::core::metainfo::Metainfo;
use backend::peer::connection::PeerContext;
use backend::peer::message::Message;
use backend::session::{Session, SessionConfig};

fn make_torrent_bytes(piece_count: usize) -> Vec<u8> {
    let piece_length: i64 = 32768;
    let total_length = piece_length as usize * piece_count;

    let mut pieces = Vec::new();
    for i in 0..piece_count {
        let data = vec![i as u8; piece_length as usize];
        let hash: [u8; 20] = sha1::Sha1::digest(&data).into();
        pieces.extend_from_slice(&hash);
    }

    let mut bencode = Vec::new();
    // d
    bencode.extend_from_slice(b"d");
    // 8:announce11:http://t.co
    bencode.extend_from_slice(b"8:announce11:http://t.co");
    // 13:creation datei0e
    bencode.extend_from_slice(b"13:creation datei0e");
    // 4:infod
    bencode.extend_from_slice(b"4:infod");
    // 6:lengthi<N>e
    bencode.extend_from_slice(b"6:lengthi");
    bencode.extend_from_slice(total_length.to_string().as_bytes());
    bencode.extend_from_slice(b"e");
    // 4:name4:test
    bencode.extend_from_slice(b"4:name4:test");
    // 12:piece lengthi<N>e
    bencode.extend_from_slice(b"12:piece lengthi");
    bencode.extend_from_slice(piece_length.to_string().as_bytes());
    bencode.extend_from_slice(b"e");
    // 6:pieces<N>:
    bencode.extend_from_slice(b"6:pieces");
    bencode.extend_from_slice(pieces.len().to_string().as_bytes());
    bencode.extend_from_slice(b":");
    bencode.extend_from_slice(&pieces);
    // ee
    bencode.extend_from_slice(b"ee");
    bencode
}

fn make_test_metainfo(piece_count: usize) -> Metainfo {
    let bytes = make_torrent_bytes(piece_count);
    Metainfo::from_bytes(&bytes).unwrap()
}

fn make_piece_data(index: u8, metainfo: &Metainfo) -> Vec<u8> {
    let piece_len = if index as usize == metainfo.piece_count() - 1 {
        metainfo.last_piece_length()
    } else {
        metainfo.info.piece_length
    };
    vec![index; piece_len]
}

#[test]
fn test_session_config_defaults() {
    let config = SessionConfig::default();
    assert_eq!(config.dht_endpoint, "http://127.0.0.1:50051");
    assert_eq!(config.peer_port, 6881);
    assert_eq!(config.max_peers, 50);
    assert_eq!(config.pipeline_depth, 5);
    assert_eq!(config.dht_refresh_interval_secs, 300);
}

#[test]
fn test_session_config_custom() {
    let config = SessionConfig {
        dht_endpoint: "http://192.168.1.1:6000".into(),
        peer_port: 9999,
        max_peers: 10,
        pipeline_depth: 3,
        dht_refresh_interval_secs: 60,
    };
    assert_eq!(config.dht_endpoint, "http://192.168.1.1:6000");
    assert_eq!(config.peer_port, 9999);
    assert_eq!(config.max_peers, 10);
    assert_eq!(config.pipeline_depth, 3);
}

#[test]
fn test_metainfo_piece_count() {
    let m = make_test_metainfo(4);
    assert_eq!(m.piece_count(), 4);
    assert_eq!(m.info.piece_length, 32768);
}

#[test]
fn test_piece_hash_deterministic() {
    let m = make_test_metainfo(1);
    let h1 = m.piece_hash(0);
    let h2 = m.piece_hash(0);
    assert_eq!(h1, h2);
}

#[test]
fn test_last_piece_length() {
    let m = make_test_metainfo(3);
    let len0 = m.piece_length_for(0);
    let len2 = m.piece_length_for(2);
    assert_eq!(len0, m.info.piece_length);
    assert_eq!(len2, m.last_piece_length());
}

#[test]
fn test_block_length_for() {
    let m = make_test_metainfo(2);
    let bl = m.block_length_for(0, 0);
    assert_eq!(bl, 16384);
    let bl = m.block_length_for(1, 16384);
    assert!(bl > 0);
    assert!(bl <= 16384);
}

#[test]
fn test_bitfield_update_interest() {
    let our = Bitfield::new(10);
    let mut peer = Bitfield::new(10);
    peer.set(0);
    peer.set(5);

    let mut ctx = PeerContext::new(
        backend::core::types::PeerId::new_random(),
        InfoHash::from_bytes([0xAA; 20]),
        10,
    );
    ctx.peer_bitfield = peer;
    ctx.update_interest(&our);
    assert!(ctx.am_interested);

    let mut our_full = Bitfield::new(10);
    for i in 0..10 {
        our_full.set(i);
    }
    ctx.update_interest(&our_full);
    assert!(!ctx.am_interested);
}

#[test]
fn test_choke_state_transitions() {
    let mut ctx = PeerContext::new(
        backend::core::types::PeerId::new_random(),
        InfoHash::from_bytes([0xBB; 20]),
        5,
    );
    assert!(ctx.am_choking);
    assert!(ctx.peer_choking);
    ctx.am_choking = false;
    ctx.peer_choking = false;
    assert!(!ctx.am_choking);
    assert!(!ctx.peer_choking);
}

#[tokio::test]
#[ignore = "requires Go DHT sidecar running on localhost:50051"]
async fn test_session_new_and_progress() {
    let metainfo = make_test_metainfo(2);
    let config = SessionConfig {
        dht_endpoint: "http://127.0.0.1:50051".into(),
        peer_port: 60001,
        max_peers: 5,
        pipeline_depth: 3,
        dht_refresh_interval_secs: 9999,
    };
    let session = Session::new(config, metainfo).await.unwrap();
    assert!((session.progress() - 0.0).abs() < f64::EPSILON);
    assert_eq!(session.metainfo().piece_count(), 2);
}

#[tokio::test]
#[ignore = "requires Go DHT sidecar running on localhost:50051"]
async fn test_session_info_hash() {
    let metainfo = make_test_metainfo(1);
    let expected_hash = metainfo.info_hash;
    let config = SessionConfig {
        dht_endpoint: "http://127.0.0.1:50051".into(),
        peer_port: 60003,
        max_peers: 5,
        pipeline_depth: 3,
        dht_refresh_interval_secs: 9999,
    };
    let session = Session::new(config, metainfo).await.unwrap();
    assert_eq!(*session.info_hash(), expected_hash);
}

#[tokio::test]
#[ignore = "requires Go DHT sidecar and a seeder peer"]
async fn test_session_download_full_flow() {
    let metainfo = make_test_metainfo(1);
    let info_hash = metainfo.info_hash;
    let expected_data = make_piece_data(0, &metainfo);
    let expected_data_clone = expected_data.clone();

    let listener = TcpListener::bind("127.0.0.1:6999")
        .await
        .expect("seeder bind");

    let seeder_task = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let mut buf = [0u8; 68];
        stream.read_exact(&mut buf).await.unwrap();

        use backend::peer::handshake::Handshake;
        let h = Handshake::new(info_hash, backend::core::types::PeerId::new_random());
        h.send(&mut stream).await.unwrap();

        let mut bf = Bitfield::new(1);
        bf.set(0);
        let msg = Message::Bitfield(bf).encode();
        stream.write_all(&msg).await.unwrap();

        let mut buf = vec![0u8; 4096];
        stream.read(&mut buf).await.unwrap();

        stream.write_all(&Message::Unchoke.encode()).await.unwrap();

        let mut buf = vec![0u8; 4096];
        stream.read(&mut buf).await.unwrap();

        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        stream
            .write_all(
                &Message::Piece {
                    index: 0,
                    begin: 0,
                    data: expected_data_clone,
                }
                .encode(),
            )
            .await
            .unwrap();

        let mut buf = vec![0u8; 4096];
        stream.read(&mut buf).await.unwrap();
    });

    {
        use backend::dht::DhtClient;
        use backend::core::types::PeerAddr;
        let mut dht =
            DhtClient::connect("http://127.0.0.1:50052").await.unwrap();
        dht.announce_peer(
            &info_hash,
            &PeerAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 6999),
        )
        .await
        .unwrap();
    }

    let config = SessionConfig {
        dht_endpoint: "http://127.0.0.1:50052".into(),
        peer_port: 60004,
        max_peers: 5,
        pipeline_depth: 3,
        dht_refresh_interval_secs: 9999,
    };

    let mut session = Session::new(config, metainfo.clone()).await.unwrap();
    let result =
        tokio::time::timeout(std::time::Duration::from_secs(30), session.download())
            .await;

    match result {
        Ok(Ok(data)) => {
            assert_eq!(data, expected_data, "downloaded data should match");
        }
        Ok(Err(e)) => {
            panic!("download failed: {}", e);
        }
        Err(_) => {
            panic!("download timed out");
        }
    }

    seeder_task.await.unwrap();
}

#[tokio::test]
async fn test_session_download_without_sidecar_fails() {
    let metainfo = make_test_metainfo(1);
    let config = SessionConfig {
        dht_endpoint: "http://127.0.0.1:19999".into(),
        peer_port: 60002,
        max_peers: 5,
        pipeline_depth: 3,
        dht_refresh_interval_secs: 9999,
    };
    let result = Session::new(config, metainfo).await;
    assert!(result.is_err(), "should fail without DHT sidecar");
}

#[test]
fn test_piece_data_sha1_verification() {
    let metainfo = make_test_metainfo(1);
    let expected_data = make_piece_data(0, &metainfo);

    let actual: [u8; 20] = sha1::Sha1::digest(&expected_data).into();
    let expected_hash = metainfo.piece_hash(0).unwrap();
    assert_eq!(actual, *expected_hash, "SHA1 must match");

    let mut bad_data = expected_data.clone();
    if !bad_data.is_empty() {
        bad_data[0] ^= 0xFF;
    }
    let bad_hash: [u8; 20] = sha1::Sha1::digest(&bad_data).into();
    assert_ne!(bad_hash, *expected_hash, "SHA1 must differ for corrupted data");
}
