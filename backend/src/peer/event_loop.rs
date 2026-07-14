use std::io;

use bytes::{Buf, BytesMut};
use tokio::net::TcpStream;
use tokio::sync::mpsc;

use crate::core::error::BError;
use crate::core::types::{PeerAddr, PeerId};
use crate::peer::message::Message;
use crate::peer::manager::{PeerCommand, PeerEvent};

pub async fn run_peer_loop(
    stream: TcpStream,
    cmd_rx: mpsc::Receiver<PeerCommand>,
    event_tx: mpsc::Sender<PeerEvent>,
    peer_addr: PeerAddr,
    peer_id: PeerId,
) {
    let mut state = PeerLoopState::new(stream, cmd_rx, event_tx, peer_addr, peer_id);
    state.run().await;
}

struct PeerLoopState {
    stream: TcpStream,
    cmd_rx: mpsc::Receiver<PeerCommand>,
    event_tx: mpsc::Sender<PeerEvent>,
    peer_addr: PeerAddr,
    peer_id: PeerId,

    read_buf: BytesMut,
    write_queue: Vec<Vec<u8>>,

    peer_choking: bool,
    peer_interested: bool,

    bytes_uploaded: u64,
    bytes_downloaded: u64,
}

impl PeerLoopState {
    fn new(
        stream: TcpStream,
        cmd_rx: mpsc::Receiver<PeerCommand>,
        event_tx: mpsc::Sender<PeerEvent>,
        peer_addr: PeerAddr,
        peer_id: PeerId,
    ) -> Self {
        Self {
            stream,
            cmd_rx,
            event_tx,
            peer_addr,
            peer_id,
            read_buf: BytesMut::with_capacity(16384),
            write_queue: Vec::new(),
            peer_choking: true,
            peer_interested: false,
            bytes_uploaded: 0,
            bytes_downloaded: 0,
        }
    }

    async fn run(&mut self) {
        loop {
            tokio::select! {
                result = self.stream.readable() => {
                    if let Err(e) = self.handle_readable(result).await {
                        if !self.is_fatal(&e) {
                            continue;
                        }
                        self.send_disconnected().await;
                        return;
                    }
                }

                cmd = self.cmd_rx.recv() => {
                    if let Err(e) = self.handle_command(cmd).await {
                        if !self.is_fatal(&e) {
                            continue;
                        }
                        self.send_disconnected().await;
                        return;
                    }
                }

                result = self.stream.writable(), if !self.write_queue.is_empty() => {
                    if let Err(e) = self.handle_writable(result).await {
                        if !self.is_fatal(&e) {
                            continue;
                        }
                        self.send_disconnected().await;
                        return;
                    }
                }
            }
        }
    }

    async fn handle_readable(
        &mut self,
        result: io::Result<()>,
    ) -> io::Result<()> {
        result?;

        match self.stream.try_read_buf(&mut self.read_buf) {
            Ok(0) => {
                return Err(io::Error::new(
                    io::ErrorKind::ConnectionReset,
                    "peer closed connection",
                ));
            }
            Ok(n) => {
                self.bytes_downloaded += n as u64;
            }
            Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => {
                return Ok(());
            }
            Err(e) => {
                return Err(e);
            }
        }

        loop {
            match Message::decode(&self.read_buf) {
                Ok((Some(msg), consumed)) => {
                    self.read_buf.advance(consumed);
                    let _ = self
                        .event_tx
                        .send(PeerEvent::ReceivedMessage {
                            peer_id: self.peer_id,
                            msg,
                        })
                        .await;
                }
                Ok((None, consumed)) => {
                    self.read_buf.advance(consumed);
                }
                Err(BError::InvalidMessage(msg)) if msg.starts_with("incomplete") => {
                    break;
                }
                Err(_) => {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        "protocol error: invalid message",
                    ));
                }
            }
        }

        if self.read_buf.is_empty() {
            self.read_buf = BytesMut::with_capacity(16384);
        }

        Ok(())
    }

    async fn handle_command(
        &mut self,
        cmd: Option<PeerCommand>,
    ) -> io::Result<()> {
        match cmd {
            Some(PeerCommand::SendMessage(msg)) => {
                self.write_queue.push(msg.encode());
                Ok(())
            }
            Some(PeerCommand::Disconnect) => {
                Err(io::Error::new(io::ErrorKind::Other, "requested disconnect"))
            }
            None => {
                Err(io::Error::new(
                    io::ErrorKind::ConnectionAborted,
                    "command channel closed",
                ))
            }
        }
    }

    async fn handle_writable(
        &mut self,
        result: io::Result<()>,
    ) -> io::Result<()> {
        result?;

        let batch: Vec<u8> = self.write_queue.iter().flatten().copied().collect();
        match self.stream.try_write(&batch) {
            Ok(n) => {
                self.bytes_uploaded += n as u64;
                self.write_queue.clear();
                Ok(())
            }
            Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => Ok(()),
            Err(e) => Err(e),
        }
    }

    fn is_fatal(&self, e: &io::Error) -> bool {
        matches!(
            e.kind(),
            io::ErrorKind::ConnectionReset
                | io::ErrorKind::ConnectionAborted
                | io::ErrorKind::BrokenPipe
                | io::ErrorKind::UnexpectedEof
                | io::ErrorKind::Other
                | io::ErrorKind::InvalidData
        )
    }

    async fn send_disconnected(&mut self) {
        let _ = self
            .event_tx
            .send(PeerEvent::Disconnected(self.peer_addr.clone()))
            .await;
    }
}
