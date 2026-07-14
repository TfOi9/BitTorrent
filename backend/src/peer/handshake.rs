use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use crate::core::error::{BError, Result};
use crate::core::types::{InfoHash, PeerId, PROTOCOL_STR, RESERVED_LEN};

#[derive(Clone, Debug)]
pub struct Handshake {
    pub info_hash: InfoHash,
    pub peer_id: PeerId,
    pub reserved: [u8; RESERVED_LEN]
}

impl Handshake {
    pub fn new(info_hash: InfoHash, peer_id: PeerId) -> Self {
        let mut reserved = [0u8; RESERVED_LEN];
        reserved[5] |= 0x10;
        reserved[7] |= 0x01;

        Self {
            info_hash,
            peer_id,
            reserved,
        }
    }

    // send a handshake to peer
    pub async fn send(&self, stream: &mut TcpStream) -> Result<()> {
        let mut buf = Vec::with_capacity(68);

        buf.push(PROTOCOL_STR.len() as u8);
        buf.extend_from_slice(PROTOCOL_STR);
        buf.extend_from_slice(&self.reserved);
        buf.extend_from_slice(self.info_hash.as_slice());
        buf.extend_from_slice(self.peer_id.as_slice());

        stream.write_all(&buf).await?;
        Ok(())
    }

    // receive and verify handshake from peer
    pub async fn receive(stream: &mut TcpStream, expected_info_hash: &InfoHash) -> Result<PeerId> {
        let mut buf = [0u8; 68];
        stream.read_exact(&mut buf).await?;

        let pstrlen = buf[0] as usize;
        if pstrlen != PROTOCOL_STR.len() {
            return Err(BError::HandshakeFailed(format!(
                "unexpected pstrlen: {} (expected {})",
                pstrlen,
                PROTOCOL_STR.len()
            )));
        }

        if &buf[1..1 + pstrlen] != PROTOCOL_STR {
            let got = String::from_utf8_lossy(&buf[1..1 + pstrlen]);
            return Err(BError::HandshakeFailed(format!(
                "protocol string mismatch: got '{}'",
                got
            )));
        }

        let _reserved = &buf[20..28];

        let received_hash = &buf[28..48];
        if received_hash != expected_info_hash.as_slice() {
            return Err(BError::HandshakeFailed(
                "info_hash mismatch: peer is serving a different torrent".into()
            ));
        }

        let mut peer_id_bytes = [0u8; 20];
        peer_id_bytes.copy_from_slice(&buf[48..68]);
        let peer_id = PeerId(peer_id_bytes);

        Ok(peer_id)
    }

    // perform bidirectional handshake
    pub async fn perform(
        stream: &mut TcpStream,
        info_hash: InfoHash,
        our_peer_id: PeerId,
    ) -> Result<PeerId> {
        let handshake = Handshake::new(info_hash, our_peer_id);
        handshake.send(stream).await?;
        Handshake::receive(stream, &info_hash).await
    }
}