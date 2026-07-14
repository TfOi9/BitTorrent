use std::fs;

use sha1::Digest;

use backend::core::metainfo::Metainfo;
use backend::storage::PieceStore;

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
    let pieces_bytes = pieces.len() * 20;
    bencode.extend(b"6:pieces");
    bencode.extend(pieces_bytes.to_string().as_bytes());
    bencode.extend(b":");
    for p in &pieces {
        bencode.extend(p);
    }
    bencode.extend(b"ee");
    bencode
}

fn make_metainfo(piece_count: usize, file_sizes: &[usize], piece_len: usize) -> Metainfo {
    let bytes = make_metainfo_bytes(piece_count, file_sizes, piece_len);
    Metainfo::from_bytes(&bytes).unwrap()
}

// ---------------------------------------------------------------------------
// Basic write and read
// ---------------------------------------------------------------------------

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
fn test_write_multiple_pieces_sequential() {
    let tmp = tempfile::tempdir().unwrap();
    let piece_count = 5;
    let piece_len = 16384;
    let total = piece_count * piece_len;
    let m = make_metainfo(piece_count, &[total], piece_len);
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
fn test_write_pieces_out_of_order() {
    let tmp = tempfile::tempdir().unwrap();
    let piece_count = 4;
    let piece_len = 16384;
    let total = piece_count * piece_len;
    let m = make_metainfo(piece_count, &[total], piece_len);
    let mut store = PieceStore::new(m, tmp.path()).unwrap();

    let order = [3, 0, 2, 1];
    for &i in &order {
        let data = vec![i as u8; piece_len];
        store.write_piece(i as u32, &data).unwrap();
    }

    for i in 0..piece_count {
        assert!(store.has_piece(i as u32));
    }
    let read = store.read_piece(0).unwrap();
    assert_eq!(read, vec![0u8; piece_len]);
    let read = store.read_piece(3).unwrap();
    assert_eq!(read, vec![3u8; piece_len]);
}

// ---------------------------------------------------------------------------
// SHA1 verification
// ---------------------------------------------------------------------------

#[test]
fn test_sha1_verification_success() {
    let tmp = tempfile::tempdir().unwrap();
    let piece_len = 16384;
    let m = make_metainfo(1, &[piece_len], piece_len);
    let mut store = PieceStore::new(m, tmp.path()).unwrap();

    let data = vec![0u8; piece_len];
    store.write_piece(0, &data).unwrap();
    assert!(store.has_piece(0));
}

#[test]
fn test_sha1_verification_fails_wrong_data() {
    let tmp = tempfile::tempdir().unwrap();
    let piece_len = 16384;
    let m = make_metainfo(1, &[piece_len], piece_len);
    let mut store = PieceStore::new(m, tmp.path()).unwrap();

    let wrong = vec![0xFFu8; piece_len];
    let result = store.write_piece(0, &wrong);
    assert!(result.is_err());
    assert!(!store.has_piece(0));
}

#[test]
fn test_sha1_verification_fails_wrong_size() {
    let tmp = tempfile::tempdir().unwrap();
    let piece_len = 16384;
    let m = make_metainfo(1, &[piece_len], piece_len);
    let mut store = PieceStore::new(m, tmp.path()).unwrap();

    let wrong = vec![0u8; piece_len + 1];
    let result = store.write_piece(0, &wrong);
    assert!(result.is_err());
}

#[test]
fn test_sha1_verification_zero_length_data() {
    let tmp = tempfile::tempdir().unwrap();
    let piece_len = 16384;
    let m = make_metainfo(1, &[piece_len], piece_len);
    let mut store = PieceStore::new(m, tmp.path()).unwrap();

    let empty = vec![];
    let result = store.write_piece(0, &empty);
    assert!(result.is_err());
}

// ---------------------------------------------------------------------------
// Block read
// ---------------------------------------------------------------------------

#[test]
fn test_read_block_first_block() {
    let tmp = tempfile::tempdir().unwrap();
    let piece_len = 65536;
    let m = make_metainfo(1, &[piece_len], piece_len);
    let mut store = PieceStore::new(m, tmp.path()).unwrap();

    let data = vec![0u8; piece_len];
    store.write_piece(0, &data).unwrap();

    let block = store.read_block(0, 0, 16384).unwrap();
    assert_eq!(block.len(), 16384);
    assert_eq!(&block, &data[..16384]);
}

#[test]
fn test_read_block_middle_block() {
    let tmp = tempfile::tempdir().unwrap();
    let piece_len = 65536;
    let m = make_metainfo(1, &[piece_len], piece_len);
    let mut store = PieceStore::new(m, tmp.path()).unwrap();

    let data = vec![0u8; piece_len];
    store.write_piece(0, &data).unwrap();

    let block = store.read_block(0, 16384, 16384).unwrap();
    assert_eq!(block.len(), 16384);
    assert_eq!(&block, &data[16384..32768]);
}

#[test]
fn test_read_block_last_block_may_be_shorter() {
    let tmp = tempfile::tempdir().unwrap();
    let piece_len = 50000;
    let m = make_metainfo(1, &[piece_len], piece_len);
    let mut store = PieceStore::new(m, tmp.path()).unwrap();

    let data = vec![0u8; piece_len];
    store.write_piece(0, &data).unwrap();

    let last_begin = (piece_len as u32 / 16384) * 16384;
    let remaining = piece_len as u32 - last_begin;
    let block = store.read_block(0, last_begin, 16384).unwrap();
    assert_eq!(block.len(), remaining as usize);
}

// ---------------------------------------------------------------------------
// Single-file torrent output
// ---------------------------------------------------------------------------

#[test]
fn test_single_file_output_correct() {
    let tmp = tempfile::tempdir().unwrap();
    let piece_len = 16384;
    let m = make_metainfo(3, &[piece_len * 3], piece_len);
    let mut store = PieceStore::new(m, tmp.path()).unwrap();

    for i in 0..3u32 {
        store.write_piece(i, &vec![i as u8; piece_len]).unwrap();
    }

    let paths = store.file_paths();
    assert_eq!(paths.len(), 1);
    assert!(paths[0].exists());

    let content = fs::read(&paths[0]).unwrap();
    assert_eq!(content.len(), piece_len * 3);
    assert_eq!(&content[..piece_len], &[0u8; 16384][..]);
    assert_eq!(&content[piece_len..piece_len * 2], &[1u8; 16384][..]);
}

#[test]
fn test_single_file_large_torrent() {
    let tmp = tempfile::tempdir().unwrap();
    let piece_len = 16384;
    let piece_count = 20;
    let total = piece_len * piece_count;
    let m = make_metainfo(piece_count, &[total], piece_len);
    let mut store = PieceStore::new(m, tmp.path()).unwrap();

    for i in 0..piece_count {
        store.write_piece(i as u32, &vec![i as u8; piece_len]).unwrap();
    }

    let paths = store.file_paths();
    let content = fs::read(&paths[0]).unwrap();
    assert_eq!(content.len(), total);
}

// ---------------------------------------------------------------------------
// Multi-file torrent output
// ---------------------------------------------------------------------------

#[test]
fn test_multi_file_two_files() {
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
fn test_multi_file_three_files() {
    let tmp = tempfile::tempdir().unwrap();
    let piece_len = 16384;
    let m = make_metainfo(3, &[16384, 16384, 16384], piece_len);
    let mut store = PieceStore::new(m, tmp.path()).unwrap();

    store.write_piece(0, &vec![0u8; piece_len]).unwrap();
    store.write_piece(1, &vec![1u8; piece_len]).unwrap();
    store.write_piece(2, &vec![2u8; piece_len]).unwrap();

    let paths = store.file_paths();
    assert_eq!(paths.len(), 3);
    for (i, path) in paths.iter().enumerate() {
        assert!(path.exists(), "file {} should exist", i);
    }
}

#[test]
fn test_multi_file_subdirectory_structure() {
    let tmp = tempfile::tempdir().unwrap();
    let piece_len = 16384;
    let m = make_metainfo(1, &[16384], piece_len);
    let mut store = PieceStore::new(m, tmp.path()).unwrap();

    store.write_piece(0, &vec![0u8; piece_len]).unwrap();

    let paths = store.file_paths();
    assert!(!paths.is_empty());
    if let Some(parent) = paths[0].parent() {
        assert!(parent.exists());
    }
}

// ---------------------------------------------------------------------------
// Piece crosses file boundary
// ---------------------------------------------------------------------------

#[test]
fn test_piece_crosses_exact_midpoint() {
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
    assert_eq!(file0, vec![0u8; 8192]);
    assert_eq!(file1, vec![0u8; 8192]);
}

#[test]
fn test_piece_crosses_uneven_boundary() {
    let tmp = tempfile::tempdir().unwrap();
    let piece_len = 16384;
    let m = make_metainfo(1, &[10000, 6384], piece_len);
    let mut store = PieceStore::new(m, tmp.path()).unwrap();

    let data = vec![0u8; piece_len];
    store.write_piece(0, &data).unwrap();

    let paths = store.file_paths();
    let file0 = fs::read(&paths[0]).unwrap();
    let file1 = fs::read(&paths[1]).unwrap();
    assert_eq!(file0.len(), 10000);
    assert_eq!(file1.len(), 6384);
}

#[test]
fn test_piece_single_byte_in_second_file() {
    let tmp = tempfile::tempdir().unwrap();
    let piece_len = 16384;
    let m = make_metainfo(1, &[16383, 1], piece_len);
    let mut store = PieceStore::new(m, tmp.path()).unwrap();

    let data = vec![0u8; piece_len];
    store.write_piece(0, &data).unwrap();

    let paths = store.file_paths();
    assert_eq!(fs::read(&paths[0]).unwrap().len(), 16383);
    assert_eq!(fs::read(&paths[1]).unwrap().len(), 1);
}

#[test]
fn test_piece_starts_at_second_file() {
    let tmp = tempfile::tempdir().unwrap();
    let piece_len = 16384;
    let m = make_metainfo(2, &[16384, 8192], piece_len);
    let mut store = PieceStore::new(m, tmp.path()).unwrap();

    store.write_piece(0, &vec![0u8; piece_len]).unwrap();
    store.write_piece(1, &vec![1u8; 8192]).unwrap();

    let paths = store.file_paths();
    let file0 = fs::read(&paths[0]).unwrap();
    let file1 = fs::read(&paths[1]).unwrap();
    assert_eq!(file0.len(), 16384);
    assert_eq!(file1.len(), 8192);

    assert_eq!(&file0, &[0u8; 16384][..]);
    assert_eq!(&file1, &[1u8; 8192][..]);
}

// ---------------------------------------------------------------------------
// Preallocate
// ---------------------------------------------------------------------------

#[test]
fn test_preallocate_creates_zero_filled_files() {
    let tmp = tempfile::tempdir().unwrap();
    let piece_len = 16384;
    let m = make_metainfo(1, &[8192, 8192], piece_len);
    let mut store = PieceStore::new(m, tmp.path()).unwrap();

    store.preallocate().unwrap();

    let paths = store.file_paths();
    for (i, path) in paths.iter().enumerate() {
        let meta = fs::metadata(path).unwrap();
        assert_eq!(meta.len(), 8192, "file {} should be preallocated", i);
        let content = fs::read(path).unwrap();
        assert!(
            content.iter().all(|&b| b == 0),
            "file {} should be zero-filled",
            i
        );
    }
}

#[test]
fn test_preallocate_then_write_overwrites() {
    let tmp = tempfile::tempdir().unwrap();
    let piece_len = 16384;
    let m = make_metainfo(1, &[piece_len], piece_len);
    let mut store = PieceStore::new(m, tmp.path()).unwrap();

    store.preallocate().unwrap();
    store.write_piece(0, &vec![0u8; piece_len]).unwrap();

    let paths = store.file_paths();
    let content = fs::read(&paths[0]).unwrap();
    assert_ne!(content, vec![0xFFu8; piece_len]);
}

#[test]
fn test_preallocate_large_file() {
    let tmp = tempfile::tempdir().unwrap();
    let piece_len = 16384;
    let piece_count = 640;
    let total = piece_len * piece_count;
    let m = make_metainfo(piece_count, &[total], piece_len);
    let mut store = PieceStore::new(m, tmp.path()).unwrap();

    store.preallocate().unwrap();

    let paths = store.file_paths();
    let meta = fs::metadata(&paths[0]).unwrap();
    assert_eq!(meta.len(), total as u64);
}

// ---------------------------------------------------------------------------
// has_piece
// ---------------------------------------------------------------------------

#[test]
fn test_has_piece_before_and_after_write() {
    let tmp = tempfile::tempdir().unwrap();
    let piece_len = 16384;
    let m = make_metainfo(2, &[piece_len * 2], piece_len);
    let mut store = PieceStore::new(m, tmp.path()).unwrap();

    assert!(!store.has_piece(0));
    assert!(!store.has_piece(1));

    store.write_piece(0, &vec![0u8; piece_len]).unwrap();
    assert!(store.has_piece(0));
    assert!(!store.has_piece(1));

    store.write_piece(1, &vec![1u8; piece_len]).unwrap();
    assert!(store.has_piece(0));
    assert!(store.has_piece(1));
}

#[test]
fn test_has_piece_with_large_index() {
    let tmp = tempfile::tempdir().unwrap();
    let piece_len = 16384;
    let piece_count = 10;
    let total = piece_len * piece_count;
    let m = make_metainfo(piece_count, &[total], piece_len);
    let mut store = PieceStore::new(m, tmp.path()).unwrap();
    assert!(!store.has_piece(9));
    store.write_piece(9, &vec![9u8; piece_len]).unwrap();
    assert!(store.has_piece(9));
    assert!(!store.has_piece(0));
}

// ---------------------------------------------------------------------------
// Last piece shorter
// ---------------------------------------------------------------------------

#[test]
fn test_last_piece_shorter_read_correct() {
    let tmp = tempfile::tempdir().unwrap();
    let piece_len = 16384;
    let last_len = 5000;
    let total = piece_len + last_len;
    let m = make_metainfo(2, &[total], piece_len);
    let mut store = PieceStore::new(m, tmp.path()).unwrap();

    store.write_piece(0, &vec![0u8; piece_len]).unwrap();
    store.write_piece(1, &vec![1u8; last_len]).unwrap();

    let read = store.read_piece(1).unwrap();
    assert_eq!(read.len(), last_len);
    assert_eq!(read, vec![1u8; last_len]);
}

#[test]
fn test_last_piece_single_byte() {
    let tmp = tempfile::tempdir().unwrap();
    let piece_len = 1;
    let m = make_metainfo(1, &[1], piece_len);
    let mut store = PieceStore::new(m, tmp.path()).unwrap();

    store.write_piece(0, &[0u8]).unwrap();
    let read = store.read_piece(0).unwrap();
    assert_eq!(read, vec![0u8]);
}

// ---------------------------------------------------------------------------
// Metainfo access
// ---------------------------------------------------------------------------

#[test]
fn test_metainfo_access() {
    let tmp = tempfile::tempdir().unwrap();
    let piece_len = 16384;
    let m = make_metainfo(3, &[piece_len * 3], piece_len);
    let store = PieceStore::new(m.clone(), tmp.path()).unwrap();
    assert_eq!(store.metainfo().piece_count(), m.piece_count());
    assert_eq!(store.metainfo().info.total_length, m.info.total_length);
}

// ---------------------------------------------------------------------------
// Error handling - invalid index
// ---------------------------------------------------------------------------

#[test]
fn test_write_piece_invalid_index() {
    let tmp = tempfile::tempdir().unwrap();
    let piece_len = 16384;
    let m = make_metainfo(1, &[piece_len], piece_len);
    let mut store = PieceStore::new(m, tmp.path()).unwrap();

    let result = store.write_piece(999, &vec![0u8; 1]);
    assert!(result.is_err());
}

// ---------------------------------------------------------------------------
// Zero-length file in multi-file torrent
// ---------------------------------------------------------------------------

#[test]
fn test_zero_length_file_created() {
    let tmp = tempfile::tempdir().unwrap();
    let piece_len = 16384;
    let m = make_metainfo(1, &[16384, 0], piece_len);
    let mut store = PieceStore::new(m, tmp.path()).unwrap();

    store.write_piece(0, &vec![0u8; piece_len]).unwrap();

    let paths = store.file_paths();
    assert_eq!(paths.len(), 2);
    assert!(paths[1].exists());
    assert_eq!(fs::read(&paths[1]).unwrap().len(), 0);
}

#[test]
fn test_zero_length_file_no_piece_span() {
    let tmp = tempfile::tempdir().unwrap();
    let piece_len = 16384;
    let m = make_metainfo(2, &[16384, 0, 16384], piece_len);
    let mut store = PieceStore::new(m, tmp.path()).unwrap();

    store.write_piece(0, &vec![0u8; piece_len]).unwrap();
    store.write_piece(1, &vec![1u8; piece_len]).unwrap();

    let paths = store.file_paths();
    assert_eq!(paths.len(), 3);
    assert_eq!(fs::read(&paths[1]).unwrap().len(), 0);
}

// ---------------------------------------------------------------------------
// Bound addr test
// ---------------------------------------------------------------------------

#[test]
fn test_detect_local_ip_returns_valid() {
    use backend::core::net_util::detect_local_ip;
    let ip = detect_local_ip();
    assert!(ip.is_ipv4());
}
