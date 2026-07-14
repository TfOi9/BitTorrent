use std::fs;
use std::net::{IpAddr, Ipv4Addr};
use std::time::Duration;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

use backend::core::bitfield::Bitfield;
use backend::core::metainfo::Metainfo;
use backend::peer::message::Message;
use backend::session::SessionConfig;
use sha1::Digest;

fn make_single_file_torrent(size: usize) -> (Metainfo, Vec<u8>) {
    let piece_len = 16384;
    let piece_count = ((size + piece_len - 1) / piece_len).max(1);

    let mut full_data = Vec::new();
    let mut pieces = Vec::new();
    for i in 0..piece_count {
        let len = if i == piece_count - 1 {
            let rem = size - i * piece_len;
            if rem == 0 { piece_len } else { rem }
        } else {
            piece_len
        };
        let data = vec![i as u8; len];
        full_data.extend_from_slice(&data);
        let hash: [u8; 20] = sha1::Sha1::digest(&data).into();
        pieces.push(hash);
    }
    full_data.truncate(size);

    let mut bencode = Vec::new();
    bencode.extend(b"d");
    bencode.extend(b"8:announce11:http://t.co");
    bencode.extend(b"4:infod");
    bencode.extend(format!("6:lengthi{}e", size).as_bytes());
    bencode.extend(b"4:name8:seedtest");
    bencode.extend(format!("12:piece lengthi{}e", piece_len).as_bytes());
    bencode.extend(b"6:pieces");
    let pieces_bytes = pieces.len() * 20;
    bencode.extend(pieces_bytes.to_string().as_bytes());
    bencode.extend(b":");
    for p in &pieces {
        bencode.extend(p);
    }
    bencode.extend(b"ee");

    let metainfo = Metainfo::from_bytes(&bencode).unwrap();
    (metainfo, full_data)
}

async fn expect_message(stream: &mut TcpStream) -> Message {
    let mut buf = vec![0u8; 65536];
    let n = tokio::time::timeout(Duration::from_secs(5), stream.read(&mut buf))
        .await
        .unwrap()
        .unwrap();
    assert!(n > 0, "connection closed");
    let (opt_msg, _) = Message::decode(&buf[..n]).unwrap();
    opt_msg.expect("expected a message, got keep-alive")
}

#[tokio::test]
async fn test_seed_responds_with_correct_piece_data() {
    let (metainfo, full_data) = make_single_file_torrent(16384);
    let info_hash = metainfo.info_hash;

    let data_dir = tempfile::tempdir().unwrap();
    let data_path = data_dir.path().join("seedtest");
    fs::write(&data_path, &full_data).unwrap();

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let seed_port = listener.local_addr().unwrap().port();

    let metainfo_clone = metainfo.clone();
    let _config = SessionConfig {
        dht_endpoint: "http://127.0.0.1:1".into(),
        bind_addr: IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)),
        peer_port: seed_port,
        max_peers: 5,
        pipeline_depth: 3,
        dht_refresh_interval_secs: 9999,
        upload_slots: 2,
    };

    let seed_handle = tokio::spawn({
        let full_data_seed = full_data.clone();
        async move {
        let (mut stream, _addr) = listener.accept().await.unwrap();

        let h = backend::peer::handshake::Handshake::new(
            info_hash,
            backend::core::types::PeerId::new_random(),
        );
        let _remote = backend::peer::handshake::Handshake::receive(&mut stream, &info_hash)
            .await
            .unwrap();
        h.send(&mut stream).await.unwrap();

        let mut bf = Bitfield::new(metainfo_clone.piece_count());
        for i in 0..metainfo_clone.piece_count() {
            bf.set(i);
        }
        stream.write_all(&Message::Bitfield(bf).encode()).await.unwrap();

        let bf_msg = expect_message(&mut stream).await;
        assert!(matches!(bf_msg, Message::Bitfield(_)));

        stream
            .write_all(&Message::Unchoke.encode())
            .await
            .unwrap();

        let req_msg = expect_message(&mut stream).await;
        if let Message::Request { index, begin, length } = req_msg {
            let piece_data = &full_data_seed[begin as usize..(begin + length) as usize];
            stream
                .write_all(
                    &Message::Piece {
                        index,
                        begin,
                        data: piece_data.to_vec(),
                    }
                    .encode(),
                )
                .await
                .unwrap();
        } else {
            panic!("expected Request, got {:?}", req_msg);
        }

        tokio::time::sleep(Duration::from_millis(300)).await;
        }
    });

    let leecher = tokio::spawn(async move {
        let mut stream =
            TcpStream::connect(format!("127.0.0.1:{}", seed_port))
                .await
                .unwrap();

        let h = backend::peer::handshake::Handshake::new(
            info_hash,
            backend::core::types::PeerId::new_random(),
        );
        h.send(&mut stream).await.unwrap();
        let _remote = backend::peer::handshake::Handshake::receive(&mut stream, &info_hash)
            .await
            .unwrap();

        let bf_msg = expect_message(&mut stream).await;
        assert!(matches!(&bf_msg, Message::Bitfield(b) if b.is_complete()));

        let empty_bf = Bitfield::new(metainfo.piece_count());
        stream
            .write_all(&Message::Bitfield(empty_bf).encode())
            .await
            .unwrap();

        let unchoke = expect_message(&mut stream).await;
        assert!(matches!(unchoke, Message::Unchoke));

        stream
            .write_all(
                &Message::Request {
                    index: 0,
                    begin: 0,
                    length: 16384,
                }
                .encode(),
            )
            .await
            .unwrap();

        let piece_msg = expect_message(&mut stream).await;
        match piece_msg {
            Message::Piece { index, begin, data } => {
                assert_eq!(index, 0);
                assert_eq!(begin, 0);
                assert_eq!(data.len(), 16384);
                assert_eq!(&data, &full_data[..16384]);
            }
            _ => panic!("expected Piece, got {:?}", piece_msg),
        }
    });

    let _ = tokio::time::timeout(Duration::from_secs(10), seed_handle).await;
    let _ = tokio::time::timeout(Duration::from_secs(10), leecher).await;
}
