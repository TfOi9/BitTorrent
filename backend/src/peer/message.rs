use bytes::{Buf, BufMut, BytesMut};
use crate::core::bitfield::Bitfield;
use crate::core::error::{BError, Result};


// A message is a byte array consisting of len(4B), id(1B) and payload(len - 1B)
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Message {
    // blocks the peer
    Choke,
    // allow the peer to request
    Unchoke,
    // interested to your data
    Interested,
    // not interested to your data
    NotInterested,
    // have a piece with #index
    Have(u32),
    // my bitfield of pieces
    Bitfield(Bitfield),
    // ask for a piece
    Request {
        index: u32,
        begin: u32,
        length: u32
    },
    // piece data transfer
    Piece {
        index: u32,
        begin: u32,
        data: Vec<u8>
    },
    // cancel a request
    Cancel {
        index: u32,
        begin: u32,
        length: u32
    }
}

impl Message {
    pub fn id(&self) -> u8 {
        match self {
            Message::Choke => 0,
            Message::Unchoke => 1,
            Message::Interested => 2,
            Message::NotInterested => 3,
            Message::Have(_) => 4,
            Message::Bitfield(_) => 5,
            Message::Request { .. } => 6,
            Message::Piece { .. } => 7,
            Message::Cancel { .. } => 8
        }
    }

    // encode a message to a byte array
    pub fn encode(&self) -> Vec<u8> {
        let mut buf = BytesMut::new();

        match self {
            Message::Choke
            | Message::Unchoke
            | Message::Interested
            | Message::NotInterested => {
                buf.put_u32(1);
                buf.put_u8(self.id());
            }
            Message::Have(index) => {
                buf.put_u32(5);
                buf.put_u8(self.id());
                buf.put_u32(*index);
            }
            Message::Bitfield(bf) => {
                let payload = bf.as_bytes();
                buf.put_u32((payload.len() + 1) as u32);
                buf.put_u8(self.id());
                buf.put_slice(payload);
            }
            Message::Request { index, begin, length } => {
                buf.put_u32(13);
                buf.put_u8(self.id());
                buf.put_u32(*index);
                buf.put_u32(*begin);
                buf.put_u32(*length);
            }
            Message::Piece { index, begin, data } => {
                buf.put_u32((data.len() + 9) as u32);
                buf.put_u8(self.id());
                buf.put_u32(*index);
                buf.put_u32(*begin);
                buf.put_slice(data);
            }
            Message::Cancel { index, begin, length } => {
                buf.put_u32(13);
                buf.put_u8(self.id());
                buf.put_u32(*index);
                buf.put_u32(*begin);
                buf.put_u32(*length);
            }
        }

        buf.to_vec()
    }

    // decode a byte array to a message
    pub fn decode(buf: &[u8]) -> Result<(Option<Message>, usize)> {
        if buf.len() < 4 {
            return Err(BError::InvalidMessage("incomplete: need 4 bytes for length prefix".into()));
        }

        let mut pos = &buf[..];
        let length = pos.get_u32() as usize;

        if length == 0 {
            return Ok((None, 4));
        }

        if buf.len() < length + 4 {
            return Err(BError::InvalidMessage(format!(
                "incomplete: need {} bytes, have {}",
                4 + length,
                buf.len()
            )));
        }

        let id = buf[4];
        let payload = &buf[5..length + 4];

        let msg = match id {
            0 => Message::Choke,
            1 => Message::Unchoke,
            2 => Message::Interested,
            3 => Message::NotInterested,
            4 => {
                if payload.len() != 4 {
                    return Err(BError::InvalidMessage(
                        "Have payload must be 4 bytes".into()
                    ));
                }
                let index = u32::from_be_bytes([payload[0], payload[1], payload[2], payload[3]]);
                Message::Have(index)
            }
            5 => {
                Message::Bitfield(Bitfield::from_bytes(payload.to_vec(), 0))
            }
            6 => {
                if payload.len() != 12 {
                    return Err(BError::InvalidMessage(
                        "Request payload must be 12 bytes".into()
                    ));
                }
                let mut c = payload;
                let index = c.get_u32();
                let begin = c.get_u32();
                let length = c.get_u32();
                Message::Request { index, begin, length }
            }
            7 => {
                if payload.len() < 8 {
                    return Err(BError::InvalidMessage(
                        "Piece payload must be at least 8 bytes".into()
                    ));
                }
                let mut c = payload;
                let index = c.get_u32();
                let begin = c.get_u32();
                let data = c.to_vec();
                Message::Piece { index, begin, data }
            }
            8 => {
                if payload.len() != 12 {
                    return Err(BError::InvalidMessage(
                        "Cancel payload must be 12 bytes".into()
                    ));
                }
                let mut c = payload;
                let index = c.get_u32();
                let begin = c.get_u32();
                let length = c.get_u32();
                Message::Cancel { index, begin, length }
            }
            _ => {
                return Err(BError::InvalidMessage(format!(
                    "unknown message id: {}", id
                )));
            }
        };

        Ok((Some(msg), length + 4))
    }
}

