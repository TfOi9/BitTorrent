use tokio::io::AsyncReadExt;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc;

use backend::core::types::{PeerAddr, PeerId};
use backend::peer::event_loop::run_peer_loop;
use backend::peer::manager::{PeerCommand, PeerEvent};
use backend::peer::message::Message;

fn make_peer_id() -> PeerId {
    PeerId::new_random()
}

#[tokio::test]
async fn test_event_loop_disconnect_on_close() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let peer_addr = PeerAddr::new(addr.ip(), addr.port());

    let (_keep_tx, cmd_rx) = mpsc::channel::<PeerCommand>(8);
    let (event_tx, mut event_rx) = mpsc::channel(8);
    let peer_id = make_peer_id();

    let server = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        drop(stream);
    });

    let client = tokio::spawn(async move {
        let stream = TcpStream::connect(addr).await.unwrap();
        run_peer_loop(stream, cmd_rx, event_tx, peer_addr, peer_id).await;
    });

    client.await.unwrap();

    let mut found_disconnect = false;
    while let Ok(event) = event_rx.try_recv() {
        if matches!(event, PeerEvent::Disconnected(_)) {
            found_disconnect = true;
        }
    }
    assert!(found_disconnect, "should receive Disconnected event");

    let _ = server.await;
}

#[tokio::test]
async fn test_event_loop_receives_message() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let peer_addr = PeerAddr::new(addr.ip(), addr.port());

    let (_keep_tx, cmd_rx) = mpsc::channel::<PeerCommand>(8);
    let (event_tx, mut event_rx) = mpsc::channel(8);
    let peer_id = make_peer_id();

    let server = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let msg = Message::Have(42).encode();
        stream.writable().await.unwrap();
        stream.try_write(&msg).unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
        drop(stream);
    });

    let client = tokio::spawn(async move {
        let stream = TcpStream::connect(addr).await.unwrap();
        run_peer_loop(stream, cmd_rx, event_tx, peer_addr, peer_id).await;
    });

    client.await.unwrap();

    let mut found_have = false;
    while let Ok(event) = event_rx.try_recv() {
        match event {
            PeerEvent::ReceivedMessage { msg, .. } => {
                if msg == Message::Have(42) {
                    found_have = true;
                }
            }
            _ => {}
        }
    }
    assert!(found_have, "should receive Have(42) message");

    let _ = server.await;
}

#[tokio::test]
async fn test_event_loop_keep_alive() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let peer_addr = PeerAddr::new(addr.ip(), addr.port());

    let (_keep_tx, cmd_rx) = mpsc::channel::<PeerCommand>(8);
    let (event_tx, mut event_rx) = mpsc::channel(8);
    let peer_id = make_peer_id();

    let server = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let keep_alive = [0u8, 0, 0, 0];
        stream.writable().await.unwrap();
        stream.try_write(&keep_alive).unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        drop(stream);
    });

    let client = tokio::spawn(async move {
        let stream = TcpStream::connect(addr).await.unwrap();
        run_peer_loop(stream, cmd_rx, event_tx, peer_addr, peer_id).await;
    });

    client.await.unwrap();

    let mut message_count = 0;
    while let Ok(event) = event_rx.try_recv() {
        if matches!(event, PeerEvent::ReceivedMessage { .. }) {
            message_count += 1;
        }
    }
    assert_eq!(message_count, 0, "keep-alive should not produce message");

    let _ = server.await;
}

#[tokio::test]
async fn test_event_loop_send_command() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let peer_addr = PeerAddr::new(addr.ip(), addr.port());

    let (cmd_tx, cmd_rx) = mpsc::channel(8);
    let (event_tx, _) = mpsc::channel(8);
    let peer_id = make_peer_id();

    let cmd_tx_send = cmd_tx.clone();
    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let mut buf = vec![0u8; 1024];
        match tokio::time::timeout(std::time::Duration::from_secs(2), stream.read(&mut buf)).await {
            Ok(Ok(n)) => assert!(n > 0, "should receive data"),
            _ => {}
        }
    });

    let client = tokio::spawn(async move {
        let stream = TcpStream::connect(addr).await.unwrap();
        cmd_tx_send
            .send(PeerCommand::SendMessage(Message::Interested))
            .await
            .unwrap();
        run_peer_loop(stream, cmd_rx, event_tx, peer_addr, peer_id).await;
    });

    let _ = tokio::time::timeout(std::time::Duration::from_secs(3), server).await;
    drop(cmd_tx);
    let _ = tokio::time::timeout(std::time::Duration::from_secs(1), client).await;
}

#[tokio::test]
async fn test_event_loop_disconnect_command() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let peer_addr = PeerAddr::new(addr.ip(), addr.port());

    let (cmd_tx, cmd_rx) = mpsc::channel(8);
    let (event_tx, mut event_rx) = mpsc::channel(8);
    let peer_id = make_peer_id();

    let _server = tokio::spawn(async move {
        let (_stream, _) = listener.accept().await.unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    });

    let client = tokio::spawn(async move {
        let stream = TcpStream::connect(addr).await.unwrap();
        cmd_tx.send(PeerCommand::Disconnect).await.unwrap();
        run_peer_loop(stream, cmd_rx, event_tx, peer_addr, peer_id).await;
    });

    client.await.unwrap();

    let mut found = false;
    while let Ok(event) = event_rx.try_recv() {
        if matches!(event, PeerEvent::Disconnected(_)) {
            found = true;
        }
    }
    assert!(found);
}

#[tokio::test]
async fn test_event_loop_multiple_messages() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let peer_addr = PeerAddr::new(addr.ip(), addr.port());

    let (cmd_tx, cmd_rx) = mpsc::channel::<PeerCommand>(8);
    let (event_tx, mut event_rx) = mpsc::channel(8);
    let peer_id = make_peer_id();

    let _cmd_tx = cmd_tx;
    let server = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let mut batch = Vec::new();
        batch.extend_from_slice(&Message::Choke.encode());
        batch.extend_from_slice(&Message::Unchoke.encode());
        batch.extend_from_slice(&Message::Interested.encode());
        stream.writable().await.unwrap();
        stream.try_write(&batch).unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
        drop(stream);
    });

    let client = tokio::spawn(async move {
        let stream = TcpStream::connect(addr).await.unwrap();
        run_peer_loop(stream, cmd_rx, event_tx, peer_addr, peer_id).await;
    });

    client.await.unwrap();

    let mut messages: Vec<Message> = Vec::new();
    while let Ok(event) = event_rx.try_recv() {
        if let PeerEvent::ReceivedMessage { msg, .. } = event {
            messages.push(msg);
        }
    }
    assert_eq!(messages.len(), 3);
    assert_eq!(messages[0], Message::Choke);
    assert_eq!(messages[1], Message::Unchoke);
    assert_eq!(messages[2], Message::Interested);

    let _ = server.await;
}

#[tokio::test]
async fn test_event_loop_incomplete_message() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let peer_addr = PeerAddr::new(addr.ip(), addr.port());

    let (_keep_tx, cmd_rx) = mpsc::channel::<PeerCommand>(8);
    let (event_tx, mut event_rx) = mpsc::channel(8);
    let peer_id = make_peer_id();

    let server = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        stream.writable().await.unwrap();
        stream.try_write(&[0, 0, 0, 100, 5, 0, 0, 0]).unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
        drop(stream);
    });

    let client = tokio::spawn(async move {
        let stream = TcpStream::connect(addr).await.unwrap();
        run_peer_loop(stream, cmd_rx, event_tx, peer_addr, peer_id).await;
    });

    client.await.unwrap();

    let mut had_panic = false;
    while let Ok(event) = event_rx.try_recv() {
        if matches!(event, PeerEvent::ReceivedMessage { .. }) {
            had_panic = true;
        }
    }
    assert!(!had_panic, "incomplete message should not be decoded");

    let _ = server.await;
}

#[tokio::test]
async fn test_event_loop_choke_filters_request() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let peer_addr = PeerAddr::new(addr.ip(), addr.port());

    let (cmd_tx, cmd_rx) = mpsc::channel(8);
    let (event_tx, _) = mpsc::channel(8);
    let peer_id = make_peer_id();

    let cmd_tx_send = cmd_tx.clone();
    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let mut buf = vec![0u8; 1024];
        match tokio::time::timeout(std::time::Duration::from_secs(2), stream.read(&mut buf)).await {
            Ok(Ok(n)) => {
                assert_eq!(n, 0, "should not send Request when choked");
            }
            _ => {}
        }
    });

    let client = tokio::spawn(async move {
        let stream = TcpStream::connect(addr).await.unwrap();
        cmd_tx_send
            .send(PeerCommand::SendMessage(Message::Request {
                index: 0,
                begin: 0,
                length: 16384,
            }))
            .await
            .unwrap();
        run_peer_loop(stream, cmd_rx, event_tx, peer_addr, peer_id).await;
    });

    let _ = tokio::time::timeout(std::time::Duration::from_secs(3), server).await;
    drop(cmd_tx);
    let _ = tokio::time::timeout(std::time::Duration::from_secs(1), client).await;
}

#[tokio::test]
async fn test_event_loop_protocol_error_detected() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let peer_addr = PeerAddr::new(addr.ip(), addr.port());

    let (_keep_tx, cmd_rx) = mpsc::channel::<PeerCommand>(8);
    let (event_tx, mut event_rx) = mpsc::channel(8);
    let peer_id = make_peer_id();

    let server = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        stream.writable().await.unwrap();
        stream.try_write(&[0, 0, 0, 1, 99]).unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
        drop(stream);
    });

    let client = tokio::spawn(async move {
        let stream = TcpStream::connect(addr).await.unwrap();
        run_peer_loop(stream, cmd_rx, event_tx, peer_addr, peer_id).await;
    });

    client.await.unwrap();

    let mut found_disconnect = false;
    while let Ok(event) = event_rx.try_recv() {
        if matches!(event, PeerEvent::Disconnected(_)) {
            found_disconnect = true;
        }
    }
    assert!(found_disconnect, "protocol error should disconnect");

    let _ = server.await;
}
