use tokio::sync::mpsc;
use crate::core::types::{PeerAddr, PeerId};
use crate::peer::connection::PeerContext;
use crate::peer::message::Message;

#[derive(Debug)]
pub enum PeerCommand {
    SendMessage(Message),
    Disconnect,
}

#[derive(Debug)]
pub enum PeerEvent {
    ReceivedMessage { peer_id: PeerId, msg: Message },
    Disconnected(PeerAddr),
    HandshakeComplete(PeerContext),
}

pub struct PeerHandle {
    pub addr: PeerAddr,
    pub cmd_tx: mpsc::Sender<PeerCommand>,
}
