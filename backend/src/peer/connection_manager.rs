use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use tokio::io::AsyncWriteExt;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc;
use tokio::time::timeout;

use crate::core::bitfield::Bitfield;
use crate::core::error::{BError, Result};
use crate::core::types::{InfoHash, PeerAddr, PeerId};
use crate::peer::connection::PeerContext;
use crate::peer::event_loop::run_peer_loop;
use crate::peer::handshake::Handshake;
use crate::peer::manager::{PeerCommand, PeerEvent, PeerHandle};
use crate::peer::message::Message;

#[derive(Clone, Debug)]
pub struct ConnectionManagerConfig {
    pub max_peers: usize,
    pub connect_timeout_secs: u64,
}

impl Default for ConnectionManagerConfig {
    fn default() -> Self {
        Self {
            max_peers: 50,
            connect_timeout_secs: 10,
        }
    }
}

pub struct NewPeerHandle {
    pub peer_id: PeerId,
    pub handle: PeerHandle,
    pub ctx: PeerContext,
}

pub struct ConnectionManager {
    peers: HashMap<PeerId, PeerHandle>,
    event_tx: mpsc::Sender<PeerEvent>,
    our_peer_id: PeerId,
    config: ConnectionManagerConfig,
    new_handles_tx: mpsc::UnboundedSender<NewPeerHandle>,
    new_handles_rx: mpsc::UnboundedReceiver<NewPeerHandle>,
}

impl ConnectionManager {
    pub fn new(
        config: ConnectionManagerConfig,
        event_tx: mpsc::Sender<PeerEvent>,
        our_peer_id: PeerId,
    ) -> Self {
        let (new_handles_tx, new_handles_rx) = mpsc::unbounded_channel();
        Self {
            peers: HashMap::new(),
            event_tx,
            our_peer_id,
            config,
            new_handles_tx,
            new_handles_rx,
        }
    }

    pub async fn connect_to_peers(
        &mut self,
        addrs: &[PeerAddr],
        info_hash: InfoHash,
        our_bitfield: &Bitfield,
    ) -> Result<usize> {
        let available = self.config.max_peers.saturating_sub(self.peers.len());
        if available == 0 {
            return Ok(0);
        }

        let mut connected = 0;
        for addr in addrs.iter().take(available) {
            if self.is_duplicate_addr(addr) {
                continue;
            }
            match self.connect_one(addr.clone(), info_hash, our_bitfield).await {
                Ok(_) => connected += 1,
                Err(e) => {
                    tracing::debug!("failed to connect to {}: {}", addr, e);
                }
            }
        }

        Ok(connected)
    }

    pub async fn start_listener(
        &mut self,
        port: u16,
        info_hash: InfoHash,
        our_bitfield: Arc<std::sync::Mutex<Bitfield>>,
    ) -> Result<tokio::task::JoinHandle<()>> {
        let listener = TcpListener::bind(format!("0.0.0.0:{}", port))
            .await
            .map_err(|e| BError::Network(format!("failed to bind port {}: {}", port, e)))?;

        let event_tx = self.event_tx.clone();
        let our_peer_id = self.our_peer_id;
        let new_handles_tx = self.new_handles_tx.clone();

        let handle = tokio::spawn(async move {
            loop {
                let (stream, peer_addr) = match listener.accept().await {
                    Ok(conn) => conn,
                    Err(e) => {
                        tracing::warn!("listener accept error: {}", e);
                        continue;
                    }
                };

                let peer_addr = PeerAddr::new(peer_addr.ip(), peer_addr.port());

                let bf = {
                    let guard = our_bitfield.lock().unwrap();
                    guard.clone()
                };

                let event_tx = event_tx.clone();
                let new_handles_tx = new_handles_tx.clone();
                tokio::spawn(async move {
                    if let Err(e) = accept_one(
                        stream,
                        peer_addr.clone(),
                        info_hash,
                        our_peer_id,
                        &bf,
                        event_tx,
                        new_handles_tx,
                    )
                    .await
                    {
                        tracing::debug!(
                            "inbound connection from {} failed: {}",
                            peer_addr,
                            e
                        );
                    }
                });
            }
        });

        Ok(handle)
    }

    pub fn drain_new_handles(&mut self) {
        while let Ok(new) = self.new_handles_rx.try_recv() {
            if self.peers.len() >= self.config.max_peers {
                let _ = new.handle.cmd_tx.send(PeerCommand::Disconnect);
                continue;
            }
            let _ = self
                .event_tx
                .send(PeerEvent::HandshakeComplete(new.ctx));
            self.peers.insert(new.peer_id, new.handle);
        }
    }

    pub async fn broadcast(&self, msg: Message) -> Result<usize> {
        let mut sent = 0;
        for handle in self.peers.values() {
            if handle
                .cmd_tx
                .send(PeerCommand::SendMessage(msg.clone()))
                .await
                .is_ok()
            {
                sent += 1;
            }
        }
        Ok(sent)
    }

    pub async fn send_to(&self, peer_id: &PeerId, msg: Message) -> Result<()> {
        let handle = self
            .peers
            .get(peer_id)
            .ok_or_else(|| BError::Network(format!("peer {} not found", peer_id)))?;

        handle
            .cmd_tx
            .send(PeerCommand::SendMessage(msg))
            .await
            .map_err(|e| BError::Network(format!("failed to send to peer {}: {}", peer_id, e)))?;

        Ok(())
    }

    pub async fn disconnect(&mut self, peer_id: &PeerId) {
        if let Some(handle) = self.peers.remove(peer_id) {
            let _ = handle.cmd_tx.send(PeerCommand::Disconnect).await;
        }
    }

    pub async fn disconnect_all(&mut self) {
        for handle in self.peers.drain().map(|(_, h)| h) {
            let _ = handle.cmd_tx.send(PeerCommand::Disconnect).await;
        }
    }

    pub fn peer_count(&self) -> usize {
        self.peers.len()
    }

    pub fn peers(&self) -> impl Iterator<Item = (&PeerId, &PeerHandle)> {
        self.peers.iter()
    }

    pub fn remove_disconnected(&mut self, peer_id: &PeerId) {
        self.peers.remove(peer_id);
    }

    fn is_duplicate_addr(&self, addr: &PeerAddr) -> bool {
        self.peers.values().any(|h| h.addr == *addr)
    }

    async fn connect_one(
        &mut self,
        addr: PeerAddr,
        info_hash: InfoHash,
        our_bitfield: &Bitfield,
    ) -> Result<()> {
        let stream = timeout(
            Duration::from_secs(self.config.connect_timeout_secs),
            TcpStream::connect(addr.to_socket_string()),
        )
        .await
        .map_err(|_| BError::Network(format!("connect to {} timed out", addr)))?
        .map_err(|e| BError::Network(format!("connect to {} failed: {}", addr, e)))?;

        let mut stream = stream;

        let remote_peer_id =
            Handshake::perform(&mut stream, info_hash, self.our_peer_id).await?;

        let bitfield_msg = Message::Bitfield(our_bitfield.clone());
        stream
            .write_all(&bitfield_msg.encode())
            .await
            .map_err(|e| BError::Network(format!("send bitfield to {} failed: {}", addr, e)))?;

        let ctx = PeerContext::new(
            remote_peer_id,
            info_hash,
            our_bitfield.total_pieces(),
        );

        let _ = self
            .event_tx
            .send(PeerEvent::HandshakeComplete(ctx))
            .await;

        let (cmd_tx, cmd_rx) = mpsc::channel(256);

        tokio::spawn(run_peer_loop(
            stream,
            cmd_rx,
            self.event_tx.clone(),
            addr.clone(),
            remote_peer_id,
        ));

        self.peers.insert(
            remote_peer_id,
            PeerHandle { addr, cmd_tx },
        );

        Ok(())
    }
}

async fn accept_one(
    mut stream: TcpStream,
    peer_addr: PeerAddr,
    info_hash: InfoHash,
    our_peer_id: PeerId,
    our_bitfield: &Bitfield,
    event_tx: mpsc::Sender<PeerEvent>,
    new_handles_tx: mpsc::UnboundedSender<NewPeerHandle>,
) -> Result<()> {
    let handshake = Handshake::new(info_hash, our_peer_id);

    let remote_peer_id = Handshake::receive(&mut stream, &info_hash).await?;

    handshake.send(&mut stream).await.map_err(|e| {
        BError::Network(format!("send handshake to {} failed: {}", peer_addr, e))
    })?;

    let bitfield_msg = Message::Bitfield(our_bitfield.clone());
    stream.write_all(&bitfield_msg.encode()).await.map_err(|e| {
        BError::Network(format!("send bitfield to {} failed: {}", peer_addr, e))
    })?;

    let ctx = PeerContext::new(remote_peer_id, info_hash, our_bitfield.total_pieces());

    let (cmd_tx, cmd_rx) = mpsc::channel(256);

    let _ = new_handles_tx.send(NewPeerHandle {
        peer_id: remote_peer_id,
        handle: PeerHandle {
            addr: peer_addr.clone(),
            cmd_tx,
        },
        ctx,
    });

    run_peer_loop(stream, cmd_rx, event_tx, peer_addr, remote_peer_id).await;

    Ok(())
}
