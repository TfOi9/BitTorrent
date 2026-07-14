use backend::core::types::{InfoHash, PeerId, PROTOCOL_STR, RESERVED_LEN};
use backend::peer::handshake::Handshake;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

fn make_info_hash(byte: u8) -> InfoHash {
    InfoHash::from_bytes([byte; 20])
}

#[test]
fn test_handshake_new_reserved_bytes() {
    let h = Handshake::new(make_info_hash(0xAA), PeerId::new_random());
    assert_eq!(h.reserved.len(), RESERVED_LEN);
    assert_eq!(h.reserved[5] & 0x10, 0x10);
    assert_eq!(h.reserved[7] & 0x01, 0x01);
}

#[test]
fn test_handshake_fields() {
    let hash = make_info_hash(0xCC);
    let peer_id = PeerId::new_random();
    let h = Handshake::new(hash, peer_id);
    assert_eq!(h.info_hash, hash);
    assert_eq!(h.peer_id, peer_id);
}

#[test]
fn test_handshake_clone() {
    let h = Handshake::new(make_info_hash(0xDD), PeerId::new_random());
    let h2 = h.clone();
    assert_eq!(h.info_hash, h2.info_hash);
    assert_eq!(h.peer_id, h2.peer_id);
    assert_eq!(h.reserved, h2.reserved);
}

#[test]
fn test_handshake_debug() {
    let h = Handshake::new(make_info_hash(0xEE), PeerId::new_random());
    let s = format!("{:?}", h);
    assert!(s.contains("Handshake"));
}

async fn spawn_mock_peer(info_hash: InfoHash, peer_id: PeerId) -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();

    tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        Handshake::receive(&mut stream, &info_hash).await.unwrap();
        let h = Handshake::new(info_hash, peer_id);
        h.send(&mut stream).await.unwrap();
    });

    port
}

async fn spawn_mock_peer_raw(info_hash: InfoHash, peer_id: PeerId) -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();

    tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let mut buf = Vec::with_capacity(68);
        buf.push(PROTOCOL_STR.len() as u8);
        buf.extend_from_slice(PROTOCOL_STR);
        buf.extend_from_slice(&[0u8; RESERVED_LEN]);
        buf.extend_from_slice(info_hash.as_slice());
        buf.extend_from_slice(peer_id.as_slice());
        stream.write_all(&buf).await.unwrap();
    });

    port
}

#[tokio::test]
async fn test_handshake_roundtrip() {
    let info_hash = make_info_hash(0xCC);
    let mock_peer_id = PeerId::new_random();
    let our_peer_id = PeerId::new_random();

    let port = spawn_mock_peer(info_hash, mock_peer_id).await;

    let mut stream = TcpStream::connect(format!("127.0.0.1:{}", port))
        .await
        .unwrap();

    let received_id = Handshake::perform(&mut stream, info_hash, our_peer_id)
        .await
        .unwrap();

    assert_eq!(received_id, mock_peer_id);
}

#[tokio::test]
async fn test_handshake_info_hash_mismatch() {
    let hash_a = make_info_hash(0xAA);
    let hash_b = make_info_hash(0xBB);
    let mock_peer_id = PeerId::new_random();

    let port = spawn_mock_peer(hash_a, mock_peer_id).await;

    let mut stream = TcpStream::connect(format!("127.0.0.1:{}", port))
        .await
        .unwrap();

    let result = Handshake::perform(&mut stream, hash_b, PeerId::new_random()).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_handshake_send_receive_separate() {
    let info_hash = make_info_hash(0x11);
    let peer_id = PeerId::new_random();

    let port = spawn_mock_peer_raw(info_hash, peer_id).await;

    let mut stream = TcpStream::connect(format!("127.0.0.1:{}", port))
        .await
        .unwrap();

    let our_h = Handshake::new(info_hash, PeerId::new_random());
    our_h.send(&mut stream).await.unwrap();

    let received_id = Handshake::receive(&mut stream, &info_hash).await.unwrap();
    assert_eq!(received_id, peer_id);
}

#[tokio::test]
async fn test_handshake_perform_returns_peer_id() {
    let info_hash = make_info_hash(0x22);
    let mock_peer_id = PeerId::new_random();

    let port = spawn_mock_peer(info_hash, mock_peer_id).await;

    let mut stream = TcpStream::connect(format!("127.0.0.1:{}", port))
        .await
        .unwrap();

    let our_peer_id = PeerId::new_random();
    let returned_id = Handshake::perform(&mut stream, info_hash, our_peer_id)
        .await
        .unwrap();

    assert_eq!(returned_id, mock_peer_id);
}

#[tokio::test]
async fn test_receive_wrong_pstrlen() {
    let info_hash = make_info_hash(0x33);
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();

    tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let mut buf = Vec::with_capacity(68);
        buf.push(30u8);
        buf.extend_from_slice(b"BitTorrent protocol extra stuff");
        buf.extend_from_slice(&[0u8; RESERVED_LEN]);
        buf.extend_from_slice(info_hash.as_slice());
        buf.extend_from_slice(PeerId::new_random().as_slice());
        stream.write_all(&buf).await.unwrap();
    });

    let mut stream = TcpStream::connect(format!("127.0.0.1:{}", port))
        .await
        .unwrap();

    let result = Handshake::receive(&mut stream, &info_hash).await;
    assert!(result.is_err());
    let err = format!("{}", result.unwrap_err());
    assert!(err.contains("pstrlen"));
}

#[tokio::test]
async fn test_receive_wrong_protocol_string() {
    let info_hash = make_info_hash(0x44);
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();

    tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let mut buf = Vec::with_capacity(68);
        let wrong_proto = b"BitTorrent protacol";
        buf.push(19u8);
        buf.extend_from_slice(wrong_proto);
        buf.extend_from_slice(&[0u8; RESERVED_LEN]);
        buf.extend_from_slice(info_hash.as_slice());
        buf.extend_from_slice(PeerId::new_random().as_slice());
        stream.write_all(&buf).await.unwrap();
    });

    let mut stream = TcpStream::connect(format!("127.0.0.1:{}", port))
        .await
        .unwrap();

    let result = Handshake::receive(&mut stream, &info_hash).await;
    assert!(result.is_err());
    let err = format!("{}", result.unwrap_err());
    assert!(err.contains("protocol string"));
}

#[tokio::test]
async fn test_receive_info_hash_mismatch() {
    let info_hash_a = make_info_hash(0xAA);
    let info_hash_b = make_info_hash(0xBB);

    let port = spawn_mock_peer_raw(info_hash_a, PeerId::new_random()).await;

    let mut stream = TcpStream::connect(format!("127.0.0.1:{}", port))
        .await
        .unwrap();

    let result = Handshake::receive(&mut stream, &info_hash_b).await;
    assert!(result.is_err());
    let err = format!("{}", result.unwrap_err());
    assert!(err.contains("info_hash"));
}

#[tokio::test]
async fn test_receive_peer_disconnects_early() {
    let info_hash = make_info_hash(0x99);
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();

    tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        stream.write_all(b"incomplete").await.unwrap();
        drop(stream);
    });

    let mut stream = TcpStream::connect(format!("127.0.0.1:{}", port))
        .await
        .unwrap();

    let result = Handshake::receive(&mut stream, &info_hash).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_handshake_wire_format() {
    let info_hash = make_info_hash(0x55);
    let peer_id = PeerId::new_random();

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();

    let h = Handshake::new(info_hash, peer_id);

    tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        h.send(&mut stream).await.unwrap();
    });

    let mut stream = TcpStream::connect(format!("127.0.0.1:{}", port))
        .await
        .unwrap();

    let mut buf = [0u8; 68];
    stream.read_exact(&mut buf).await.unwrap();

    assert_eq!(buf[0], 19);
    assert_eq!(&buf[1..20], PROTOCOL_STR);
    assert_eq!(&buf[20..28], &[0, 0, 0, 0, 0, 0x10, 0, 0x01]);
    assert_eq!(&buf[28..48], info_hash.as_slice());
    assert_eq!(&buf[48..68], peer_id.as_slice());
}

#[tokio::test]
async fn test_multiple_sequential_handshakes() {
    for i in 0..5 {
        let info_hash = make_info_hash(0x60 + i);
        let mock_peer_id = PeerId::new_random();
        let our_peer_id = PeerId::new_random();

        let port = spawn_mock_peer(info_hash, mock_peer_id).await;

        let mut stream = TcpStream::connect(format!("127.0.0.1:{}", port))
            .await
            .unwrap();

        let received_id = Handshake::perform(&mut stream, info_hash, our_peer_id)
            .await
            .unwrap();

        assert_eq!(received_id, mock_peer_id);
    }
}

#[tokio::test]
async fn test_handshake_send_then_receive_from_peer() {
    let info_hash = make_info_hash(0x77);
    let our_peer_id = PeerId::new_random();
    let their_peer_id = PeerId::new_random();

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();

    tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let _ = Handshake::receive(&mut stream, &info_hash).await;
        let h = Handshake::new(info_hash, their_peer_id);
        h.send(&mut stream).await.unwrap();
    });

    let mut stream = TcpStream::connect(format!("127.0.0.1:{}", port))
        .await
        .unwrap();

    let h = Handshake::new(info_hash, our_peer_id);
    h.send(&mut stream).await.unwrap();

    let received_id = Handshake::receive(&mut stream, &info_hash).await.unwrap();
    assert_eq!(received_id, their_peer_id);
}

#[tokio::test]
async fn test_handshake_different_info_hashes_per_connection() {
    let hash1 = make_info_hash(0x11);
    let hash2 = make_info_hash(0x22);

    let port1 = spawn_mock_peer_raw(hash1, PeerId::new_random()).await;
    let port2 = spawn_mock_peer_raw(hash2, PeerId::new_random()).await;

    let mut s1 = TcpStream::connect(format!("127.0.0.1:{}", port1)).await.unwrap();
    let mut s2 = TcpStream::connect(format!("127.0.0.1:{}", port2)).await.unwrap();

    let id1 = Handshake::receive(&mut s1, &hash1).await.unwrap();
    let id2 = Handshake::receive(&mut s2, &hash2).await.unwrap();

    assert_ne!(id1, id2);
}
