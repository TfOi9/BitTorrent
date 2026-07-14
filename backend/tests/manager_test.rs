use backend::peer::manager::{PeerCommand, PeerEvent, PeerHandle};
use backend::peer::message::Message;
use backend::core::types::PeerAddr;
use std::net::{IpAddr, Ipv4Addr};
use tokio::sync::mpsc;

#[test]
fn test_peer_command_debug() {
    let cmd = PeerCommand::SendMessage(Message::Choke);
    assert!(format!("{:?}", cmd).contains("Choke"));
    let cmd = PeerCommand::Disconnect;
    assert_eq!(format!("{:?}", cmd), "Disconnect");
}

#[test]
fn test_peer_event_debug() {
    let event = PeerEvent::ReceivedMessage(Message::Have(42));
    assert!(format!("{:?}", event).contains("Have"));
    let addr = PeerAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 6881);
    let event = PeerEvent::Disconnected(addr.clone());
    assert!(format!("{:?}", event).contains("127.0.0.1"));
}

#[test]
fn test_peer_handle_send_and_recv() {
    let (tx, mut rx) = mpsc::channel::<PeerCommand>(8);
    let addr = PeerAddr::new(IpAddr::V4(Ipv4Addr::new(192, 168, 1, 1)), 9999);
    let handle = PeerHandle {
        addr: addr.clone(),
        cmd_tx: tx,
    };
    assert_eq!(handle.addr, addr);
    tokio::runtime::Runtime::new()
        .unwrap()
        .block_on(async {
            handle
                .cmd_tx
                .send(PeerCommand::SendMessage(Message::Interested))
                .await
                .unwrap();
            let cmd = rx.recv().await.unwrap();
            match cmd {
                PeerCommand::SendMessage(msg) => assert_eq!(msg, Message::Interested),
                _ => panic!("expected SendMessage"),
            }
        });
}

#[test]
fn test_peer_handle_disconnect() {
    let (tx, mut rx) = mpsc::channel::<PeerCommand>(8);
    let addr = PeerAddr::new(IpAddr::V4(Ipv4Addr::new(10, 0, 0, 1)), 8080);
    let handle = PeerHandle { addr, cmd_tx: tx };
    tokio::runtime::Runtime::new()
        .unwrap()
        .block_on(async {
            handle.cmd_tx.send(PeerCommand::Disconnect).await.unwrap();
            let cmd = rx.recv().await.unwrap();
            match cmd {
                PeerCommand::Disconnect => {}
                _ => panic!("expected Disconnect"),
            }
        });
}

#[test]
fn test_peer_handle_drop_closes_channel() {
    let (tx, mut rx) = mpsc::channel::<PeerCommand>(8);
    let addr = PeerAddr::new(IpAddr::V4(Ipv4Addr::new(1, 2, 3, 4)), 1234);
    let handle = PeerHandle { addr, cmd_tx: tx };
    drop(handle);
    tokio::runtime::Runtime::new()
        .unwrap()
        .block_on(async {
            let result = rx.recv().await;
            assert!(result.is_none());
        });
}
