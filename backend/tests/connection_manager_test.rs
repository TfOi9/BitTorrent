use std::net::{IpAddr, Ipv4Addr};
use std::sync::Arc;

use tokio::io::AsyncReadExt;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc;

use backend::core::bitfield::Bitfield;
use backend::core::types::{InfoHash, PeerAddr, PeerId};
use backend::peer::connection_manager::{
    ConnectionManager, ConnectionManagerConfig,
};
use backend::peer::handshake::Handshake;
use backend::peer::manager::PeerEvent;
use backend::peer::message::Message;

fn make_info_hash(v: u8) -> InfoHash {
    InfoHash::from_bytes([v; 20])
}

fn make_bitfield(pieces: usize) -> Bitfield {
    Bitfield::new(pieces)
}

async fn spawn_seeder(listener: TcpListener, info_hash: InfoHash, peer_id: PeerId) {
    let (mut stream, _) = listener.accept().await.unwrap();
    let handshake = Handshake::new(info_hash, peer_id);
    handshake.send(&mut stream).await.unwrap();
    Handshake::receive(&mut stream, &info_hash).await.unwrap();
    // expect bitfield
    let mut buf = vec![0u8; 1024];
    let _ = stream.read(&mut buf).await;
}

#[tokio::test]
async fn test_connect_to_single_peer() {
    let info_hash = make_info_hash(0x01);
    let seeder_id = PeerId::new_random();

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let peer_addr = PeerAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), port);

    let (event_tx, mut event_rx) = mpsc::channel(8);
    let our_id = PeerId::new_random();
    let mut cm = ConnectionManager::new(
        ConnectionManagerConfig::default(),
        event_tx,
        our_id,
    );

    let bf = make_bitfield(10);

    let seeder = tokio::spawn(spawn_seeder(listener, info_hash, seeder_id));

    let connected = cm
        .connect_to_peers(&[peer_addr], info_hash, &bf)
        .await
        .unwrap();

    assert_eq!(connected, 1);
    assert_eq!(cm.peer_count(), 1);

    let mut found_handshake = false;
    while let Ok(event) = event_rx.try_recv() {
        if matches!(event, PeerEvent::HandshakeComplete(_)) {
            found_handshake = true;
        }
    }
    assert!(found_handshake);

    cm.disconnect_all().await;
    seeder.await.unwrap();
}

#[tokio::test]
async fn test_connect_to_multiple_peers() {
    let info_hash = make_info_hash(0x02);
    let bf = make_bitfield(10);
    let (event_tx, _) = mpsc::channel(16);
    let our_id = PeerId::new_random();
    let mut cm = ConnectionManager::new(
        ConnectionManagerConfig::default(),
        event_tx,
        our_id,
    );

    let mut addrs = Vec::new();
    let mut servers = Vec::new();

    for _ in 0..3 {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        addrs.push(PeerAddr::new(
            IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)),
            port,
        ));

        let ih = info_hash;
        let pid = PeerId::new_random();
        servers.push(tokio::spawn(spawn_seeder(
            listener, ih, pid,
        )));
    }

    let connected = cm
        .connect_to_peers(&addrs, info_hash, &bf)
        .await
        .unwrap();

    assert_eq!(connected, 3);
    assert_eq!(cm.peer_count(), 3);

    cm.disconnect_all().await;
    for s in servers {
        s.await.unwrap();
    }
}

#[tokio::test]
async fn test_broadcast_to_peers() {
    let info_hash = make_info_hash(0x03);
    let bf = make_bitfield(10);
    let (event_tx, _) = mpsc::channel(16);
    let our_id = PeerId::new_random();
    let mut cm = ConnectionManager::new(
        ConnectionManagerConfig::default(),
        event_tx,
        our_id,
    );

    let (done_tx, mut done_rx) = mpsc::channel(2);
    let mut addrs = Vec::new();
    let mut servers = Vec::new();

    for _ in 0..2 {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        addrs.push(PeerAddr::new(
            IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)),
            port,
        ));

        let ih = info_hash;
        let pid = PeerId::new_random();
        let dt = done_tx.clone();
        servers.push(tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let h = Handshake::new(ih, pid);
            h.send(&mut stream).await.unwrap();
            Handshake::receive(&mut stream, &ih).await.unwrap();

            let mut buf = vec![0u8; 4096];
            let mut have_found = false;
            loop {
                match tokio::time::timeout(
                    std::time::Duration::from_secs(3),
                    stream.read(&mut buf),
                )
                .await
                {
                    Ok(Ok(n)) if n > 0 => {
                        let mut pos = 0;
                        let data = buf[..n].to_vec();
                        while pos < data.len() {
                            match backend::peer::message::Message::decode(&data[pos..]) {
                                Ok((Some(backend::peer::message::Message::Have(7)), consumed)) => {
                                    have_found = true;
                                    pos += consumed;
                                }
                                Ok((_, consumed)) => {
                                    pos += consumed;
                                }
                                Err(_) => {
                                    break;
                                }
                            }
                        }
                        if have_found {
                            break;
                        }
                    }
                    _ => break,
                }
            }
            assert!(have_found, "should receive Have(7) broadcast");
            dt.send(()).await.unwrap();
        }));
    }

    cm.connect_to_peers(&addrs, info_hash, &bf)
        .await
        .unwrap();

    // give event loops time to start
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    let sent = cm.broadcast(Message::Have(7)).await.unwrap();
    assert_eq!(sent, 2);

    drop(done_tx);
    let mut ack = 0;
    while let Ok(Some(())) = tokio::time::timeout(
        std::time::Duration::from_secs(5),
        done_rx.recv(),
    )
    .await
    {
        ack += 1;
    }
    assert_eq!(ack, 2, "both peers should receive broadcast");

    cm.disconnect_all().await;
    for s in servers {
        let _ = s.await;
    }
}

#[tokio::test]
async fn test_disconnect_peer() {
    let info_hash = make_info_hash(0x04);
    let seeder_id = PeerId::new_random();

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let peer_addr = PeerAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), port);

    let (event_tx, _) = mpsc::channel(8);
    let our_id = PeerId::new_random();
    let mut cm = ConnectionManager::new(
        ConnectionManagerConfig::default(),
        event_tx,
        our_id,
    );

    let bf = make_bitfield(10);
    let seeder = tokio::spawn(spawn_seeder(listener, info_hash, seeder_id));

    cm.connect_to_peers(&[peer_addr], info_hash, &bf)
        .await
        .unwrap();
    assert_eq!(cm.peer_count(), 1);

    let peer_ids: Vec<PeerId> = cm.peers().map(|(id, _)| *id).collect();
    cm.disconnect(&peer_ids[0]).await;
    assert_eq!(cm.peer_count(), 0);

    seeder.await.unwrap();
}

#[tokio::test]
async fn test_max_peers_limit() {
    let info_hash = make_info_hash(0x05);
    let bf = make_bitfield(10);
    let (event_tx, _) = mpsc::channel(16);
    let our_id = PeerId::new_random();
    let mut cm = ConnectionManager::new(
        ConnectionManagerConfig {
            max_peers: 2,
            connect_timeout_secs: 5,
        },
        event_tx,
        our_id,
    );

    let mut addrs = Vec::new();
    let mut listeners = Vec::new();

    for _ in 0..5 {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        addrs.push(PeerAddr::new(
            IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)),
            port,
        ));
        listeners.push(listener);
    }

    // only spawn seeders for the first 3 (just in case 3rd connects)
    let mut servers = Vec::new();
    for (i, listener) in listeners.into_iter().enumerate() {
        let ih = info_hash;
        let pid = PeerId::new_random();
        if i < 3 {
            servers.push(tokio::spawn(spawn_seeder(listener, ih, pid)));
        }
    }

    let connected = cm
        .connect_to_peers(&addrs, info_hash, &bf)
        .await
        .unwrap();

    assert!(connected <= 2);
    assert!(cm.peer_count() <= 2);

    cm.disconnect_all().await;
    for s in servers {
        let _ = tokio::time::timeout(
            std::time::Duration::from_secs(2),
            s,
        ).await;
    }
}

#[tokio::test]
async fn test_listener_drain_new_handles() {
    let info_hash = make_info_hash(0x06);
    let (event_tx, mut event_rx) = mpsc::channel(16);
    let our_id = PeerId::new_random();
    let mut cm = ConnectionManager::new(
        ConnectionManagerConfig::default(),
        event_tx.clone(),
        our_id,
    );

    let bf = Arc::new(std::sync::Mutex::new(make_bitfield(10)));

    let listener_port = {
        let tmp = TcpListener::bind("127.0.0.1:0").await.unwrap();
        tmp.local_addr().unwrap().port()
    };

    let _handle = cm
        .start_listener(listener_port, info_hash, bf)
        .await
        .unwrap();

    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    let client_id = PeerId::new_random();
    let mut stream =
        TcpStream::connect(format!("127.0.0.1:{}", listener_port))
            .await
            .unwrap();

    let h = Handshake::new(info_hash, client_id);
    h.send(&mut stream).await.unwrap();
    let _remote_id = Handshake::receive(&mut stream, &info_hash).await.unwrap();

    // read bitfield from the listener
    let mut buf = vec![0u8; 1024];
    let _ = tokio::time::timeout(
        std::time::Duration::from_secs(3),
        stream.read(&mut buf),
    )
    .await;

    drop(stream);

    let mut found = false;
    for _ in 0..50 {
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        cm.drain_new_handles();
        while let Ok(event) = event_rx.try_recv() {
            if matches!(event, PeerEvent::HandshakeComplete(_)) {
                found = true;
            }
        }
        if cm.peer_count() > 0 {
            found = true;
            break;
        }
    }
    assert!(found, "inbound peer should be registered via drain_new_handles");

    cm.disconnect_all().await;
}

#[tokio::test]
async fn test_duplicate_addr_skipped() {
    let info_hash = make_info_hash(0x07);
    let seeder_id = PeerId::new_random();

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let peer_addr = PeerAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), port);

    let (event_tx, _) = mpsc::channel(8);
    let our_id = PeerId::new_random();
    let mut cm = ConnectionManager::new(
        ConnectionManagerConfig::default(),
        event_tx,
        our_id,
    );

    let bf = make_bitfield(10);
    let seeder = tokio::spawn(spawn_seeder(listener, info_hash, seeder_id));

    let connected = cm
        .connect_to_peers(&[peer_addr.clone(), peer_addr], info_hash, &bf)
        .await
        .unwrap();

    assert_eq!(connected, 1);
    assert_eq!(cm.peer_count(), 1);

    cm.disconnect_all().await;
    seeder.await.unwrap();
}

#[tokio::test]
async fn test_send_to_specific_peer() {
    let info_hash = make_info_hash(0x08);
    let bf = make_bitfield(10);
    let (event_tx, _) = mpsc::channel(16);
    let our_id = PeerId::new_random();
    let mut cm = ConnectionManager::new(
        ConnectionManagerConfig::default(),
        event_tx,
        our_id,
    );

    let (done_tx, mut done_rx) = mpsc::channel(2);
    let mut addrs = Vec::new();
    let mut peer_ids = Vec::new();
    let mut servers = Vec::new();

    for _ in 0..2 {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        addrs.push(PeerAddr::new(
            IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)),
            port,
        ));

        let ih = info_hash;
        let pid = PeerId::new_random();
        peer_ids.push(pid);
        let dt = done_tx.clone();
        servers.push(tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let h = Handshake::new(ih, pid);
            h.send(&mut stream).await.unwrap();
            Handshake::receive(&mut stream, &ih).await.unwrap();
            let mut buf = vec![0u8; 1024];
            let _ = stream.read(&mut buf).await;
            // second read for the targeted message (only one peer gets it)
            match tokio::time::timeout(
                std::time::Duration::from_secs(2),
                stream.read(&mut buf),
            )
            .await
            {
                Ok(Ok(n)) if n > 0 => {
                    dt.send(1u32).await.unwrap();
                }
                _ => {
                    dt.send(0u32).await.unwrap();
                }
            }
        }));
    }

    cm.connect_to_peers(&addrs, info_hash, &bf)
        .await
        .unwrap();

    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    let actual_ids: Vec<PeerId> = cm.peers().map(|(id, _)| *id).collect();
    cm.send_to(&actual_ids[0], Message::Interested)
        .await
        .unwrap();

    tokio::time::sleep(std::time::Duration::from_millis(500)).await;

    drop(done_tx);
    let mut received = 0u32;
    loop {
        match tokio::time::timeout(
            std::time::Duration::from_secs(2),
            done_rx.recv(),
        )
        .await
        {
            Ok(Some(v)) => received += v,
            _ => break,
        }
    }
    assert_eq!(received, 1, "only one peer should receive targeted message");

    cm.disconnect_all().await;
    for s in servers {
        s.await.unwrap();
    }
}

#[tokio::test]
async fn test_remove_disconnected() {
    let info_hash = make_info_hash(0x09);
    let seeder_id = PeerId::new_random();

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let peer_addr = PeerAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), port);

    let (event_tx, _) = mpsc::channel(8);
    let our_id = PeerId::new_random();
    let mut cm = ConnectionManager::new(
        ConnectionManagerConfig::default(),
        event_tx,
        our_id,
    );

    let bf = make_bitfield(10);
    let seeder = tokio::spawn(spawn_seeder(listener, info_hash, seeder_id));

    cm.connect_to_peers(&[peer_addr], info_hash, &bf)
        .await
        .unwrap();
    assert_eq!(cm.peer_count(), 1);

    let peer_ids: Vec<PeerId> = cm.peers().map(|(id, _)| *id).collect();
    cm.remove_disconnected(&peer_ids[0]);
    assert_eq!(cm.peer_count(), 0);

    cm.disconnect_all().await;
    seeder.await.unwrap();
}
