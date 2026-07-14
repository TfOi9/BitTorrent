use backend::core::bitfield::Bitfield;
use backend::peer::message::Message;

#[test]
fn test_keep_alive() {
    let buf = [0u8; 4];
    let (msg, consumed) = Message::decode(&buf).unwrap();
    assert!(msg.is_none());
    assert_eq!(consumed, 4);
}

#[test]
fn test_choke_roundtrip() {
    let encoded = Message::Choke.encode();
    assert_eq!(encoded, vec![0, 0, 0, 1, 0]);
    let (decoded, _) = Message::decode(&encoded).unwrap();
    assert_eq!(decoded, Some(Message::Choke));
}

#[test]
fn test_unchoke_roundtrip() {
    let encoded = Message::Unchoke.encode();
    assert_eq!(encoded, vec![0, 0, 0, 1, 1]);
    let (decoded, _) = Message::decode(&encoded).unwrap();
    assert_eq!(decoded, Some(Message::Unchoke));
}

#[test]
fn test_interested_roundtrip() {
    let encoded = Message::Interested.encode();
    assert_eq!(encoded, vec![0, 0, 0, 1, 2]);
    let (decoded, _) = Message::decode(&encoded).unwrap();
    assert_eq!(decoded, Some(Message::Interested));
}

#[test]
fn test_not_interested_roundtrip() {
    let encoded = Message::NotInterested.encode();
    assert_eq!(encoded, vec![0, 0, 0, 1, 3]);
    let (decoded, _) = Message::decode(&encoded).unwrap();
    assert_eq!(decoded, Some(Message::NotInterested));
}

#[test]
fn test_have_roundtrip() {
    let encoded = Message::Have(42).encode();
    let (decoded, _) = Message::decode(&encoded).unwrap();
    assert_eq!(decoded, Some(Message::Have(42)));
}

#[test]
fn test_request_roundtrip() {
    let encoded = Message::Request {
        index: 0,
        begin: 16384,
        length: 16384,
    }
    .encode();
    let (decoded, _) = Message::decode(&encoded).unwrap();
    assert_eq!(
        decoded,
        Some(Message::Request {
            index: 0,
            begin: 16384,
            length: 16384,
        })
    );
}

#[test]
fn test_request_roundtrip_high_values() {
    let encoded = Message::Request {
        index: 9999,
        begin: 1_000_000,
        length: 16384,
    }
    .encode();
    let (decoded, _) = Message::decode(&encoded).unwrap();
    assert_eq!(
        decoded,
        Some(Message::Request {
            index: 9999,
            begin: 1_000_000,
            length: 16384,
        })
    );
}

#[test]
fn test_small_piece_roundtrip() {
    let data = b"hello world".to_vec();
    let encoded = Message::Piece {
        index: 3,
        begin: 0,
        data: data.clone(),
    }
    .encode();
    let (decoded, _) = Message::decode(&encoded).unwrap();
    assert_eq!(
        decoded,
        Some(Message::Piece {
            index: 3,
            begin: 0,
            data,
        })
    );
}

#[test]
fn test_incomplete_message() {
    let buf = [0, 0, 0, 10, 1];
    let result = Message::decode(&buf);
    assert!(result.is_err());
}

#[test]
fn test_unknown_message_id() {
    let buf = [0, 0, 0, 1, 99];
    let result = Message::decode(&buf);
    assert!(result.is_err());
}

#[test]
fn test_cancel_roundtrip() {
    let encoded = Message::Cancel {
        index: 7,
        begin: 32768,
        length: 8192,
    }
    .encode();
    let (decoded, _) = Message::decode(&encoded).unwrap();
    assert_eq!(
        decoded,
        Some(Message::Cancel {
            index: 7,
            begin: 32768,
            length: 8192,
        })
    );
}

#[test]
fn test_bitfield_encode_structure() {
    let bf = Bitfield::new(16);
    let encoded = Message::Bitfield(bf.clone()).encode();
    let payload_len = (bf.as_bytes().len() + 1) as u32;
    assert_eq!(&encoded[..4], &payload_len.to_be_bytes());
    assert_eq!(encoded[4], 5);
    assert_eq!(&encoded[5..], bf.as_bytes());
}

#[test]
fn test_bitfield_roundtrip_with_data() {
    let mut bf = Bitfield::new(16);
    bf.set(0);
    bf.set(3);
    bf.set(15);
    let encoded = Message::Bitfield(bf.clone()).encode();
    let payload_len = (bf.as_bytes().len() + 1) as u32;
    assert_eq!(&encoded[..4], &payload_len.to_be_bytes());
    assert_eq!(encoded[4], 5);
    assert_eq!(&encoded[5..], bf.as_bytes());
}

#[test]
fn test_piece_empty_data_roundtrip() {
    let encoded = Message::Piece {
        index: 0,
        begin: 0,
        data: vec![],
    }
    .encode();
    let (decoded, _) = Message::decode(&encoded).unwrap();
    assert_eq!(
        decoded,
        Some(Message::Piece {
            index: 0,
            begin: 0,
            data: vec![],
        })
    );
}

#[test]
fn test_piece_large_data_roundtrip() {
    let data = vec![0xAAu8; 16384];
    let encoded = Message::Piece {
        index: 5,
        begin: 0,
        data: data.clone(),
    }
    .encode();
    let (decoded, _) = Message::decode(&encoded).unwrap();
    assert_eq!(
        decoded,
        Some(Message::Piece {
            index: 5,
            begin: 0,
            data,
        })
    );
}

#[test]
fn test_message_id_method() {
    assert_eq!(Message::Choke.id(), 0);
    assert_eq!(Message::Unchoke.id(), 1);
    assert_eq!(Message::Interested.id(), 2);
    assert_eq!(Message::NotInterested.id(), 3);
    assert_eq!(Message::Have(0).id(), 4);
    assert_eq!(Message::Bitfield(Bitfield::new(1)).id(), 5);
    assert_eq!(Message::Request { index: 0, begin: 0, length: 0 }.id(), 6);
    assert_eq!(Message::Piece { index: 0, begin: 0, data: vec![] }.id(), 7);
    assert_eq!(Message::Cancel { index: 0, begin: 0, length: 0 }.id(), 8);
}

#[test]
fn test_have_boundary_values() {
    let encoded = Message::Have(0).encode();
    let (decoded, _) = Message::decode(&encoded).unwrap();
    assert_eq!(decoded, Some(Message::Have(0)));

    let encoded = Message::Have(u32::MAX).encode();
    let (decoded, _) = Message::decode(&encoded).unwrap();
    assert_eq!(decoded, Some(Message::Have(u32::MAX)));
}

#[test]
fn test_request_boundary_values() {
    let encoded = Message::Request {
        index: u32::MAX,
        begin: u32::MAX,
        length: u32::MAX,
    }
    .encode();
    let (decoded, _) = Message::decode(&encoded).unwrap();
    assert_eq!(
        decoded,
        Some(Message::Request {
            index: u32::MAX,
            begin: u32::MAX,
            length: u32::MAX,
        })
    );
}

#[test]
fn test_cancel_boundary_values() {
    let encoded = Message::Cancel {
        index: u32::MAX,
        begin: 0,
        length: u32::MAX,
    }
    .encode();
    let (decoded, _) = Message::decode(&encoded).unwrap();
    assert_eq!(
        decoded,
        Some(Message::Cancel {
            index: u32::MAX,
            begin: 0,
            length: u32::MAX,
        })
    );
}

#[test]
fn test_decode_empty_buffer() {
    let result = Message::decode(&[]);
    assert!(result.is_err());
}

#[test]
fn test_decode_truncated_length() {
    for len in 1..=3 {
        let buf = vec![0u8; len];
        let result = Message::decode(&buf);
        assert!(result.is_err(), "expected error for buffer length {}", len);
    }
}

#[test]
fn test_decode_have_invalid_payload() {
    let buf = [0, 0, 0, 3, 4, 0xFF];
    let result = Message::decode(&buf);
    assert!(result.is_err());
}

#[test]
fn test_decode_request_invalid_payload() {
    let buf = [0, 0, 0, 10, 6, 0, 0, 0, 1, 0, 0, 0, 2, 0xFF];
    let result = Message::decode(&buf);
    assert!(result.is_err());
}

#[test]
fn test_decode_piece_minimal_payload() {
    let buf = [0, 0, 0, 9, 7, 0, 0, 0, 0, 0, 0, 0, 0];
    let (decoded, _) = Message::decode(&buf).unwrap();
    assert_eq!(
        decoded,
        Some(Message::Piece {
            index: 0,
            begin: 0,
            data: vec![],
        })
    );
}

#[test]
fn test_multiple_messages_decode() {
    let mut buf = Vec::new();
    buf.extend_from_slice(&Message::Choke.encode());
    buf.extend_from_slice(&Message::Unchoke.encode());
    buf.extend_from_slice(&Message::Have(10).encode());

    let (msg, n1) = Message::decode(&buf).unwrap();
    assert_eq!(msg, Some(Message::Choke));
    let (msg, n2) = Message::decode(&buf[n1..]).unwrap();
    assert_eq!(msg, Some(Message::Unchoke));
    let (msg, _n3) = Message::decode(&buf[n1 + n2..]).unwrap();
    assert_eq!(msg, Some(Message::Have(10)));
}

#[test]
fn test_all_message_types_encode_length_prefix() {
    let messages = vec![
        Message::Choke,
        Message::Unchoke,
        Message::Interested,
        Message::NotInterested,
        Message::Have(1),
        Message::Bitfield(Bitfield::new(8)),
        Message::Request { index: 0, begin: 0, length: 0 },
        Message::Piece { index: 0, begin: 0, data: vec![1, 2, 3] },
        Message::Cancel { index: 0, begin: 0, length: 0 },
    ];
    for msg in &messages {
        let encoded = msg.encode();
        assert!(encoded.len() >= 4);
        let declared_len =
            u32::from_be_bytes([encoded[0], encoded[1], encoded[2], encoded[3]]) as usize;
        assert_eq!(declared_len + 4, encoded.len());
    }
}

#[test]
fn test_request_all_zeros_roundtrip() {
    let encoded = Message::Request {
        index: 0,
        begin: 0,
        length: 0,
    }
    .encode();
    let (decoded, _) = Message::decode(&encoded).unwrap();
    assert_eq!(
        decoded,
        Some(Message::Request {
            index: 0,
            begin: 0,
            length: 0,
        })
    );
}

#[test]
fn test_cancel_all_zeros_roundtrip() {
    let encoded = Message::Cancel {
        index: 0,
        begin: 0,
        length: 0,
    }
    .encode();
    let (decoded, _) = Message::decode(&encoded).unwrap();
    assert_eq!(
        decoded,
        Some(Message::Cancel {
            index: 0,
            begin: 0,
            length: 0,
        })
    );
}

#[test]
fn test_decode_incomplete_at_id() {
    let buf = [0, 0, 0, 2];
    let result = Message::decode(&buf);
    assert!(result.is_err());
}

#[test]
fn test_piece_various_data_sizes() {
    for size in [0, 1, 256, 1024, 16384] {
        let data = vec![0x42u8; size];
        let encoded = Message::Piece {
            index: 1,
            begin: 1024,
            data: data.clone(),
        }
        .encode();
        let (decoded, _) = Message::decode(&encoded).unwrap();
        assert_eq!(
            decoded,
            Some(Message::Piece {
                index: 1,
                begin: 1024,
                data,
            })
        );
    }
}

#[test]
fn test_message_encode_len_id_consistency() {
    let messages = [
        Message::Choke,
        Message::Unchoke,
        Message::Interested,
        Message::NotInterested,
    ];
    for (i, msg) in messages.iter().enumerate() {
        let encoded = msg.encode();
        assert_eq!(encoded.len(), 5);
        assert_eq!(encoded[3], 1);
        assert_eq!(encoded[4], i as u8);
    }
}
