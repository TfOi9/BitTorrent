use backend::core::metainfo::Metainfo;
use backend::storage::PieceStore;
use sha1::Digest;
use std::fs;

fn make_metainfo_bytes(
    piece_count: usize,
    file_sizes: &[usize],
    piece_len: usize,
) -> Vec<u8> {
    let total_length: usize = file_sizes.iter().sum();
    let mut pieces = Vec::new();
    for i in 0..piece_count {
        let len = if i == piece_count - 1 {
            total_length - i * piece_len
        } else {
            piece_len
        };
        let data = vec![i as u8; len];
        let hash: [u8; 20] = sha1::Sha1::digest(&data).into();
        pieces.push(hash);
    }

    let mut bencode = Vec::new();
    bencode.extend(b"d");
    bencode.extend(b"8:announce11:http://t.co");
    if file_sizes.len() == 1 {
        bencode.extend(b"4:infod");
        bencode.extend(format!("6:lengthi{}e", file_sizes[0]).as_bytes());
        bencode.extend(b"4:name4:test");
    } else {
        bencode.extend(b"4:infod");
        bencode.extend(b"4:name8:test_dir");
        bencode.extend(b"5:filesl");
        for (i, &size) in file_sizes.iter().enumerate() {
            let filename = format!("file{}.dat", i);
            bencode.extend(b"d");
            bencode.extend(format!("6:lengthi{}e", size).as_bytes());
            bencode.extend(b"4:pathl");
            bencode.extend(format!("{}:{}", filename.len(), filename).as_bytes());
            bencode.extend(b"e");
            bencode.extend(b"e");
        }
        bencode.extend(b"e");
    }
    bencode.extend(format!("12:piece lengthi{}e", piece_len).as_bytes());
    bencode.extend(b"6:pieces");
    let pieces_bytes = pieces.len() * 20;
    bencode.extend(pieces_bytes.to_string().as_bytes());
    bencode.extend(b":");
    for p in &pieces {
        bencode.extend(p);
    }
    bencode.extend(b"ee");
    bencode
}

fn make_metainfo(
    piece_count: usize,
    file_sizes: &[usize],
    piece_len: usize,
) -> Metainfo {
    let bytes = make_metainfo_bytes(piece_count, file_sizes, piece_len);
    Metainfo::from_bytes(&bytes).unwrap()
}

#[test]
fn test_write_and_read_single_piece() {
    let tmp = tempfile::tempdir().unwrap();
    let m = make_metainfo(1, &[16384], 16384);
    let mut store = PieceStore::new(m, tmp.path()).unwrap();

    let data = vec![0u8; 16384];
    store.write_piece(0, &data).unwrap();
    assert!(store.has_piece(0));

    let read = store.read_piece(0).unwrap();
    assert_eq!(read, data);
}

#[test]
fn test_sha1_verification_fails() {
    let tmp = tempfile::tempdir().unwrap();
    let m = make_metainfo(1, &[16384], 16384);
    let mut store = PieceStore::new(m, tmp.path()).unwrap();

    let wrong = vec![0xFFu8; 16384];
    let result = store.write_piece(0, &wrong);
    assert!(result.is_err());
    assert!(!store.has_piece(0));
}

#[test]
fn test_write_multiple_pieces() {
    let tmp = tempfile::tempdir().unwrap();
    let piece_count = 3;
    let piece_len = 16384;
    let m = make_metainfo(piece_count, &[piece_len * piece_count], piece_len);
    let mut store = PieceStore::new(m, tmp.path()).unwrap();

    for i in 0..piece_count {
        let data = vec![i as u8; piece_len];
        store.write_piece(i as u32, &data).unwrap();
    }

    for i in 0..piece_count {
        assert!(store.has_piece(i as u32));
        let expected = vec![i as u8; piece_len];
        let read = store.read_piece(i as u32).unwrap();
        assert_eq!(read, expected);
    }
}

#[test]
fn test_read_block_partial() {
    let tmp = tempfile::tempdir().unwrap();
    let piece_len = 32768;

    let mut data = vec![0u8; piece_len];
    for (i, b) in data.iter_mut().enumerate() {
        *b = (i % 256) as u8;
    }

    let m = {
        let hash: [u8; 20] = sha1::Sha1::digest(&data).into();
        let mut bencode = Vec::new();
        bencode.extend(b"d");
        bencode.extend(b"8:announce11:http://t.co");
        bencode.extend(b"4:infod");
        bencode.extend(format!("6:lengthi{}e", piece_len).as_bytes());
        bencode.extend(b"4:name4:test");
        bencode.extend(format!("12:piece lengthi{}e", piece_len).as_bytes());
        bencode.extend(b"6:pieces20:");
        bencode.extend(&hash);
        bencode.extend(b"ee");
        Metainfo::from_bytes(&bencode).unwrap()
    };
    let mut store = PieceStore::new(m, tmp.path()).unwrap();

    store.write_piece(0, &data).unwrap();

    let block = store.read_block(0, 0, 16384).unwrap();
    assert_eq!(block.len(), 16384);
    assert_eq!(&block, &data[..16384]);

    let block = store.read_block(0, 16384, 16384).unwrap();
    assert_eq!(block.len(), 16384);
    assert_eq!(&block, &data[16384..]);
}

#[test]
fn test_single_file_output() {
    let tmp = tempfile::tempdir().unwrap();
    let piece_len = 16384;
    let m = make_metainfo(2, &[piece_len * 2], piece_len);
    let mut store = PieceStore::new(m, tmp.path()).unwrap();

    store.write_piece(0, &vec![0u8; piece_len]).unwrap();
    store.write_piece(1, &vec![1u8; piece_len]).unwrap();

    let paths = store.file_paths();
    assert_eq!(paths.len(), 1);
    let output_file = &paths[0];
    assert!(output_file.exists());
    let file_data = fs::read(output_file).unwrap();
    assert_eq!(file_data.len(), piece_len * 2);
    assert_eq!(&file_data[..piece_len], &[0u8; 16384]);
    assert_eq!(&file_data[piece_len..], &[1u8; 16384]);
}

#[test]
fn test_multi_file_output() {
    let tmp = tempfile::tempdir().unwrap();
    let piece_len = 16384;
    let m = make_metainfo(2, &[16384, 16384], piece_len);
    let mut store = PieceStore::new(m, tmp.path()).unwrap();

    store.write_piece(0, &vec![0u8; piece_len]).unwrap();
    store.write_piece(1, &vec![1u8; piece_len]).unwrap();

    let paths = store.file_paths();
    assert_eq!(paths.len(), 2);

    let data0 = fs::read(&paths[0]).unwrap();
    let data1 = fs::read(&paths[1]).unwrap();
    assert_eq!(data0, vec![0u8; 16384]);
    assert_eq!(data1, vec![1u8; 16384]);
}

#[test]
fn test_piece_crosses_file_boundary() {
    let tmp = tempfile::tempdir().unwrap();
    let piece_len = 16384;
    let m = make_metainfo(1, &[8192, 8192], piece_len);
    let mut store = PieceStore::new(m, tmp.path()).unwrap();

    let data = vec![0u8; piece_len];
    store.write_piece(0, &data).unwrap();

    let paths = store.file_paths();
    let file0 = fs::read(&paths[0]).unwrap();
    let file1 = fs::read(&paths[1]).unwrap();
    assert_eq!(file0.len(), 8192);
    assert_eq!(file1.len(), 8192);
    assert_eq!(&file0, &data[..8192]);
    assert_eq!(&file1, &data[8192..]);
}

#[test]
fn test_preallocate() {
    let tmp = tempfile::tempdir().unwrap();
    let piece_len = 16384;
    let m = make_metainfo(1, &[8192, 8192], piece_len);
    let mut store = PieceStore::new(m, tmp.path()).unwrap();

    store.preallocate().unwrap();

    let paths = store.file_paths();
    for (i, path) in paths.iter().enumerate() {
        let meta = fs::metadata(path).unwrap();
        assert_eq!(meta.len(), 8192);
        let content = fs::read(path).unwrap();
        assert!(content.iter().all(|&b| b == 0), "file {} should be zero-filled", i);
    }
}

#[test]
fn test_has_piece() {
    let tmp = tempfile::tempdir().unwrap();
    let piece_len = 16384;
    let m = make_metainfo(3, &[piece_len * 3], piece_len);
    let mut store = PieceStore::new(m, tmp.path()).unwrap();

    store.write_piece(0, &vec![0u8; piece_len]).unwrap();
    assert!(store.has_piece(0));
    assert!(!store.has_piece(1));
    assert!(!store.has_piece(2));

    store.write_piece(2, &vec![2u8; piece_len]).unwrap();
    assert!(store.has_piece(2));
    assert!(!store.has_piece(1));
}

#[test]
fn test_last_piece_shorter_correct_length() {
    let tmp = tempfile::tempdir().unwrap();
    let piece_len = 16384;
    let last_len = 8192;
    let total = piece_len + last_len;
    let m = make_metainfo(2, &[total], piece_len);
    let mut store = PieceStore::new(m, tmp.path()).unwrap();

    store.write_piece(0, &vec![0u8; piece_len]).unwrap();
    store.write_piece(1, &vec![1u8; last_len]).unwrap();

    let read = store.read_piece(1).unwrap();
    assert_eq!(read.len(), last_len);
    assert_eq!(read, vec![1u8; last_len]);
}
