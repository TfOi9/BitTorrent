use thiserror::Error;

#[derive(Error, Debug)]
pub enum BError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Bencode parse error: {0}")]
    BencodeParse(String),

    #[error("Bencode: unexpected end of input")]
    BencodeUnexpectedEof,

    #[error("Bencode: expected integer, got {0:?}")]
    BencodeExpectedInteger(u8),

    #[error("Bencode: expected byte string, got invalid length prefix")]
    BencodeInvalidLength,

    #[error("Bencode: expected colon separator after length prefix")]
    BencodeMissingColon,

    #[error("Invalid torrent file: {0}")]
    InvalidTorrent(String),

    #[error("Torrent file is missing required 'info' dictionary")]
    MissingInfoDict,

    #[error("Torrent file is missing required 'pieces' field")]
    MissingPieces,

    #[error("Torrent 'pieces' field has invalid length: expected multiple of 20, got {0}")]
    InvalidPiecesLength(usize),

    #[error("Torrent file has no files (neither 'length' nor 'files' field in info)")]
    NoFiles,

    #[error("Torrent 'piece length' must be positive, got {0}")]
    InvalidPieceLength(i64),

    #[error("Invalid info_hash: {0}")]
    InvalidInfoHash(String),

    #[error("Invalid peer address: {0}")]
    InvalidPeerAddr(String),

    #[error("Hex decode error: {0}")]
    HexDecode(#[from] hex::FromHexError),

    #[error("Network error: {0}")]
    Network(String),

    #[error("Handshake failed: {0}")]
    HandshakeFailed(String),

    #[error("Invalid message: {0}")]
    InvalidMessage(String),

    #[error("gRPC error: {0}")]
    Grpc(#[from] tonic::Status),

    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),

    #[error("Session error: {0}")]
    Session(String),
}

pub type Result<T> = std::result::Result<T, BError>;