use std::fmt;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

// SHA-1 hash has 20 bytes
pub const SHA1_LEN: usize = 20;

// Every piece has a length of 16 KiB
pub const BLOCK_LEN: u32 = 16 * 1024;

// Peer Wire Protocol Handshake's Identifier
pub const PROTOCOL_STR: &[u8; 19] = b"BitTorrent protocol";

// Handshake's reserves 8 bytes
pub const RESERVED_LEN: usize = 8;


// InfoHash: Torrent's indetifier, SHA1(bencode(info_dict))
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct InfoHash(pub [u8; SHA1_LEN]);

impl InfoHash {
    #[inline]
    pub fn from_bytes(bytes: [u8; SHA1_LEN]) -> Self {
        Self(bytes)
    }

    pub fn from_hex(hex: &str) -> Result<Self, crate::core::error::BError> {
        let bytes = hex::decode(hex)?;
        if bytes.len() != SHA1_LEN {
            return Err(crate::core::error::BError::InvalidInfoHash(
                format!("expected 40 hex chars (20 bytes), got {} chars", hex.len())
            ));
        }
        let mut arr = [0u8; SHA1_LEN];
        arr.copy_from_slice(&bytes);
        Ok(Self(arr))
    }

    #[inline]
    pub fn as_bytes(&self) -> &[u8; SHA1_LEN] {
        &self.0
    }

    #[inline]
    pub fn as_slice(&self) -> &[u8] {
        &self.0
    }

    pub fn short_hex(&self) -> String {
        format!("{:02x}{:02x}...", self.0[0], self.0[1])
    }
}

impl fmt::Display for InfoHash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", hex::encode(self.0))
    }
}

impl fmt::Debug for InfoHash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "InfoHash({})", hex::encode(&self.0[..4]))
    }
}

impl From<[u8; SHA1_LEN]> for InfoHash {
    fn from(bytes: [u8; SHA1_LEN]) -> Self {
        Self(bytes)
    }
}

// PeerId: BitTorrent's client's identifier
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct PeerId(pub [u8; SHA1_LEN]);

impl PeerId {
    pub fn new_random() -> Self {
        use rand::RngCore;
        let mut id = [0u8; SHA1_LEN];
        id[..8].copy_from_slice(b"-RB0001-");
        rand::thread_rng().fill_bytes(&mut id[8..]);
        Self(id)
    }

    #[inline]
    pub fn as_slice(&self) -> &[u8] {
        &self.0
    }
}

impl fmt::Display for PeerId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Ok(s) = std::str::from_utf8(&self.0) {
            write!(f, "{}", s)
        } else {
            write!(f, "{}", hex::encode(self.0))
        }
    }
}

impl fmt::Debug for PeerId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "PeerId({})", self)
    }
}

// PieceIndex: 0-based index for pieces
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct PieceIndex(pub u32);

impl PieceIndex {
    #[inline]
    pub fn as_usize(&self) -> usize {
        self.0 as usize
    }
}

impl fmt::Display for PieceIndex {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

// BlockOffset: a block's offset in a piece
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct BlockOffset(pub u32);

// BlockLength: a block's length in bytes
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct BlockLength(pub u32);

// BlockRequest: a block's full information
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct BlockRequest {
    pub index: PieceIndex,
    pub begin: BlockOffset,
    pub length: BlockLength,
}

impl BlockRequest {
    pub fn new(index: PieceIndex, begin: BlockOffset, length: BlockLength) -> Self {
        Self { index, begin, length }
    }
}

// PeerAddr: a peer's address including IP and port
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct PeerAddr {
    pub ip: IpAddr,
    pub port: u16,
}

impl PeerAddr {
    pub fn new(ip: IpAddr, port: u16) -> Self {
        Self{ip, port}
    }

    pub fn from_compact_v4(bytes: &[u8]) -> Result<Self, crate::core::error::BError> {
        if bytes.len() != 6 {
            return Err(crate::core::error::BError::InvalidPeerAddr(
                "compact peer must be 6 bytes".into()
            ));
        }
        let ip = Ipv4Addr::new(bytes[0], bytes[1], bytes[2], bytes[3]);
        let port = u16::from_be_bytes([bytes[4], bytes[5]]);
        Ok(Self{ip: IpAddr::V4(ip), port})
    }

    pub fn from_compact_v6(bytes: &[u8]) -> Result<Self, crate::core::error::BError> {
        if bytes.len() != 18 {
            return Err(crate::core::error::BError::InvalidPeerAddr(
                "compact IPv6 peer must be 18 bytes".into()
            ));
        }
        let mut ip_bytes = [0u8; 16];
        ip_bytes.copy_from_slice(&bytes[..16]);
        let ip = Ipv6Addr::from(ip_bytes);
        let port = u16::from_be_bytes([bytes[16], bytes[17]]);
        Ok(Self { ip: IpAddr::V6(ip), port })
    }

    pub fn to_socket_string(&self) -> String {
        format!("{}:{}", self.ip, self.port)
    }
}

impl fmt::Display for PeerAddr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}", self.ip, self.port)
    }
}

