use backend::core::metainfo::Metainfo;
use backend::core::bencode::BencodeValue;
use std::collections::BTreeMap;

const SHA1_LEN: usize = 20;

fn sha1_pieces(count: usize) -> Vec<u8> {
    let mut v = Vec::with_capacity(count * SHA1_LEN);
    for i in 0..count {
        let mut hash = [0u8; SHA1_LEN];
        hash[0..8].copy_from_slice(&(i as u64).to_be_bytes());
        v.extend_from_slice(&hash);
    }
    v
}

fn build_dict(entries: Vec<(Vec<u8>, BencodeValue)>) -> BencodeValue {
    BencodeValue::Dict(entries.into_iter().collect::<BTreeMap<_, _>>())
}

fn b_str(s: &str) -> BencodeValue {
    BencodeValue::ByteString(s.as_bytes().to_vec())
}

fn b_int(i: i64) -> BencodeValue {
    BencodeValue::Integer(i)
}

fn b_list(items: Vec<BencodeValue>) -> BencodeValue {
    BencodeValue::List(items)
}

fn make_torrent_bytes(root_entries: Vec<(Vec<u8>, BencodeValue)>) -> Vec<u8> {
    build_dict(root_entries).encode()
}

fn make_single_file_info(name: &str, length: i64, piece_length: i64, piece_count: usize) -> BencodeValue {
    build_dict(vec![
        (b"piece length".to_vec(), b_int(piece_length)),
        (b"pieces".to_vec(), BencodeValue::ByteString(sha1_pieces(piece_count))),
        (b"name".to_vec(), b_str(name)),
        (b"length".to_vec(), b_int(length)),
    ])
}

fn make_multi_file_info(name: &str, files: Vec<(&str, i64)>, piece_length: i64, piece_count: usize) -> BencodeValue {
    let file_entries: Vec<BencodeValue> = files
        .into_iter()
        .map(|(path, len)| {
            let segments: Vec<BencodeValue> = path.split('/').map(|s| b_str(s)).collect();
            build_dict(vec![
                (b"length".to_vec(), b_int(len)),
                (b"path".to_vec(), b_list(segments)),
            ])
        })
        .collect();
    build_dict(vec![
        (b"piece length".to_vec(), b_int(piece_length)),
        (b"pieces".to_vec(), BencodeValue::ByteString(sha1_pieces(piece_count))),
        (b"name".to_vec(), b_str(name)),
        (b"files".to_vec(), b_list(file_entries)),
    ])
}

fn single_file_torrent(name: &str, length: i64, piece_length: i64) -> Vec<u8> {
    let piece_count = ((length as usize + piece_length as usize - 1) / piece_length as usize).max(1);
    let info = make_single_file_info(name, length, piece_length, piece_count);
    make_torrent_bytes(vec![
        (b"announce".to_vec(), b_str("http://tracker.example.com/ann")),
        (b"info".to_vec(), info),
    ])
}

#[test]
fn test_parse_single_file() {
    let data = single_file_torrent("test.txt", 1024, 262144);
    let meta = Metainfo::from_bytes(&data).unwrap();

    assert_eq!(meta.announce.as_deref(), Some("http://tracker.example.com/ann"));
    assert_eq!(meta.info.name, "test.txt");
    assert_eq!(meta.info.piece_length, 262144);
    assert_eq!(meta.info.total_length, 1024);
    assert_eq!(meta.piece_count(), 1);
    assert!(meta.is_single_file());
    assert!(!meta.is_multi_file());
    assert_eq!(meta.info.files.len(), 1);
    assert_eq!(meta.info.files[0].path, vec!["test.txt"]);
    assert_eq!(meta.info.files[0].length, 1024);
}

#[test]
fn test_info_hash_is_computed() {
    let data = single_file_torrent("test.txt", 1024, 262144);
    let meta = Metainfo::from_bytes(&data).unwrap();
    assert_eq!(meta.info_hash.as_bytes().len(), 20);
}

#[test]
fn test_info_hash_deterministic() {
    let data = single_file_torrent("test.txt", 1024, 262144);
    let h1 = Metainfo::from_bytes(&data).unwrap().info_hash;
    let h2 = Metainfo::from_bytes(&data).unwrap().info_hash;
    assert_eq!(h1, h2);
}

#[test]
fn test_info_hash_different_for_different_content() {
    let d1 = single_file_torrent("a.txt", 100, 256);
    let d2 = single_file_torrent("b.txt", 200, 256);
    let h1 = Metainfo::from_bytes(&d1).unwrap().info_hash;
    let h2 = Metainfo::from_bytes(&d2).unwrap().info_hash;
    assert_ne!(h1, h2);
}

#[test]
fn test_parse_multi_file_torrent() {
    let info = make_multi_file_info(
        "mydir",
        vec![("readme.txt", 100), ("src/main.rs", 500), ("data.bin", 1024)],
        256,
        7,
    );
    let data = make_torrent_bytes(vec![
        (b"announce".to_vec(), b_str("http://t.example.com/ann")),
        (b"info".to_vec(), info),
    ]);
    let meta = Metainfo::from_bytes(&data).unwrap();

    assert_eq!(meta.info.name, "mydir");
    assert_eq!(meta.info.total_length, 1624);
    assert!(!meta.is_single_file());
    assert!(meta.is_multi_file());
    assert_eq!(meta.info.files.len(), 3);
    assert_eq!(meta.info.files[0].path, vec!["readme.txt"]);
    assert_eq!(meta.info.files[0].length, 100);
    assert_eq!(meta.info.files[1].path, vec!["src", "main.rs"]);
    assert_eq!(meta.info.files[1].length, 500);
    assert_eq!(meta.info.files[2].path, vec!["data.bin"]);
    assert_eq!(meta.info.files[2].length, 1024);
}

#[test]
fn test_parse_multi_file_deep_paths() {
    let info = make_multi_file_info(
        "deep",
        vec![("a/b/c/d/e/file.txt", 42)],
        256,
        1,
    );
    let data = make_torrent_bytes(vec![
        (b"announce".to_vec(), b_str("http://t.example.com/ann")),
        (b"info".to_vec(), info),
    ]);
    let meta = Metainfo::from_bytes(&data).unwrap();
    assert_eq!(meta.info.files[0].path, vec!["a", "b", "c", "d", "e", "file.txt"]);
}

#[test]
fn test_parse_many_files() {
    let files: Vec<(&str, i64)> = (0..200)
        .map(|i| {
            let path = Box::leak(format!("file_{:03}.dat", i).into_boxed_str());
            (path as &str, (i * 10) as i64)
        })
        .collect();
    let total: i64 = files.iter().map(|(_, l)| l).sum();
    let piece_length = 512;
    let piece_count = ((total as usize + piece_length as usize - 1) / piece_length as usize).max(1);
    let info = make_multi_file_info("many_files", files, piece_length, piece_count);
    let data = make_torrent_bytes(vec![
        (b"info".to_vec(), info),
    ]);
    let meta = Metainfo::from_bytes(&data).unwrap();
    assert_eq!(meta.info.files.len(), 200);
    assert_eq!(meta.info.total_length, total as usize);
}

#[test]
fn test_parse_without_announce() {
    let info = make_single_file_info("x.bin", 100, 256, 1);
    let data = make_torrent_bytes(vec![(b"info".to_vec(), info)]);
    let meta = Metainfo::from_bytes(&data).unwrap();
    assert_eq!(meta.announce, None);
    assert_eq!(meta.announce_list, None);
    assert!(meta.all_tracker_urls().is_empty());
}

#[test]
fn test_parse_with_announce_only() {
    let info = make_single_file_info("x.bin", 100, 256, 1);
    let data = make_torrent_bytes(vec![
        (b"announce".to_vec(), b_str("http://a.com/ann")),
        (b"info".to_vec(), info),
    ]);
    let meta = Metainfo::from_bytes(&data).unwrap();
    assert_eq!(meta.announce.as_deref(), Some("http://a.com/ann"));
    assert_eq!(meta.all_tracker_urls(), vec!["http://a.com/ann"]);
}

#[test]
fn test_parse_with_announce_list_single_tier() {
    let info = make_single_file_info("x.bin", 100, 256, 1);
    let announce_list = b_list(vec![
        b_list(vec![b_str("http://t1.com/ann"), b_str("http://t2.com/ann")]),
    ]);
    let data = make_torrent_bytes(vec![
        (b"announce".to_vec(), b_str("http://a.com/ann")),
        (b"announce-list".to_vec(), announce_list),
        (b"info".to_vec(), info),
    ]);
    let meta = Metainfo::from_bytes(&data).unwrap();
    assert_eq!(meta.announce.as_deref(), Some("http://a.com/ann"));
    let list = meta.announce_list.as_ref().unwrap();
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].len(), 2);
}

#[test]
fn test_parse_with_announce_list_multi_tier() {
    let info = make_single_file_info("x.bin", 100, 256, 1);
    let announce_list = b_list(vec![
        b_list(vec![b_str("udp://t1.com:80")]),
        b_list(vec![b_str("http://t2.com/ann"), b_str("http://t3.com/ann")]),
        b_list(vec![b_str("udp://t4.com:8080")]),
    ]);
    let data = make_torrent_bytes(vec![
        (b"info".to_vec(), info),
        (b"announce-list".to_vec(), announce_list),
    ]);
    let meta = Metainfo::from_bytes(&data).unwrap();
    let list = meta.announce_list.as_ref().unwrap();
    assert_eq!(list.len(), 3);
    assert_eq!(list[0].len(), 1);
    assert_eq!(list[1].len(), 2);
    assert_eq!(list[2].len(), 1);
}

#[test]
fn test_all_tracker_urls_dedup() {
    let info = make_single_file_info("x.bin", 100, 256, 1);
    let announce_list = b_list(vec![
        b_list(vec![b_str("http://a.com/ann")]),
    ]);
    let data = make_torrent_bytes(vec![
        (b"announce".to_vec(), b_str("http://a.com/ann")),
        (b"announce-list".to_vec(), announce_list),
        (b"info".to_vec(), info),
    ]);
    let meta = Metainfo::from_bytes(&data).unwrap();
    assert_eq!(meta.all_tracker_urls().len(), 1);
}

#[test]
fn test_parse_with_all_optional_fields() {
    let info = make_single_file_info("full.bin", 2048, 1024, 2);
    let data = make_torrent_bytes(vec![
        (b"announce".to_vec(), b_str("http://a.com/ann")),
        (b"comment".to_vec(), b_str("test comment")),
        (b"created by".to_vec(), b_str("test-harness")),
        (b"creation date".to_vec(), b_int(1600000000)),
        (b"encoding".to_vec(), b_str("UTF-8")),
        (b"info".to_vec(), info),
    ]);
    let meta = Metainfo::from_bytes(&data).unwrap();
    assert_eq!(meta.comment.as_deref(), Some("test comment"));
    assert_eq!(meta.created_by.as_deref(), Some("test-harness"));
    assert_eq!(meta.creation_date, Some(1600000000));
    assert_eq!(meta.encoding.as_deref(), Some("UTF-8"));
}

#[test]
fn test_single_file_zero_length() {
    let info = make_single_file_info("empty.dat", 0, 16384, 0);
    let data = make_torrent_bytes(vec![
        (b"announce".to_vec(), b_str("http://t.com/ann")),
        (b"info".to_vec(), info),
    ]);
    let meta = Metainfo::from_bytes(&data).unwrap();
    assert_eq!(meta.info.total_length, 0);
    assert_eq!(meta.piece_count(), 0);
    assert_eq!(meta.info.files[0].length, 0);
    assert!(meta.piece_hash(0).is_none());
}

#[test]
fn test_total_length_exactly_divisible_by_piece_length() {
    let data = single_file_torrent("data.bin", 1024, 256);
    let meta = Metainfo::from_bytes(&data).unwrap();
    assert_eq!(meta.piece_count(), 4);
    assert_eq!(meta.last_piece_length(), 256);
}

#[test]
fn test_total_length_not_divisible_by_piece_length() {
    let data = single_file_torrent("data.bin", 1000, 256);
    let meta = Metainfo::from_bytes(&data).unwrap();
    assert_eq!(meta.piece_count(), 4);
    assert_eq!(meta.last_piece_length(), 1000 % 256);
}

#[test]
fn test_single_piece_torrent() {
    let data = single_file_torrent("tiny.dat", 1, 262144);
    let meta = Metainfo::from_bytes(&data).unwrap();
    assert_eq!(meta.piece_count(), 1);
    assert_eq!(meta.last_piece_length(), 1);
}

#[test]
fn test_large_file() {
    let length: i64 = 10i64 * 1024 * 1024 * 1024;
    let piece_length: i64 = 2 * 1024 * 1024;
    let piece_count = ((length as usize + piece_length as usize - 1) / piece_length as usize).max(1);
    let info = make_single_file_info("huge.bin", length, piece_length, piece_count);
    let data = make_torrent_bytes(vec![
        (b"announce".to_vec(), b_str("http://t.com/ann")),
        (b"info".to_vec(), info),
    ]);
    let meta = Metainfo::from_bytes(&data).unwrap();
    assert_eq!(meta.info.total_length, length as usize);
    assert_eq!(meta.piece_count(), piece_count);
    assert!(meta.is_single_file());
}

#[test]
fn test_many_pieces() {
    let piece_count = 500;
    let piece_length: i64 = 16384;
    let length = piece_length * piece_count as i64;
    let info = make_single_file_info("many_pieces.bin", length, piece_length, piece_count);
    let data = make_torrent_bytes(vec![
        (b"info".to_vec(), info),
    ]);
    let meta = Metainfo::from_bytes(&data).unwrap();
    assert_eq!(meta.piece_count(), piece_count);
    assert_eq!(meta.info.total_length, length as usize);
}

#[test]
fn test_piece_hash_access() {
    let piece_count = 5;
    let info = make_single_file_info("data.bin", 500, 100, piece_count);
    let data = make_torrent_bytes(vec![
        (b"info".to_vec(), info),
    ]);
    let meta = Metainfo::from_bytes(&data).unwrap();
    assert!(meta.piece_hash(0).is_some());
    assert!(meta.piece_hash(4).is_some());
    assert!(meta.piece_hash(5).is_none());
    assert!(meta.piece_hash(999).is_none());
}

#[test]
fn test_piece_length_for_first_and_middle() {
    let data = single_file_torrent("data.bin", 1000, 256);
    let meta = Metainfo::from_bytes(&data).unwrap();
    assert_eq!(meta.piece_length_for(0), 256);
    assert_eq!(meta.piece_length_for(1), 256);
    assert_eq!(meta.piece_length_for(2), 256);
}

#[test]
fn test_piece_length_for_last() {
    let data = single_file_torrent("data.bin", 1000, 256);
    let meta = Metainfo::from_bytes(&data).unwrap();
    let last_idx = meta.piece_count() - 1;
    assert_eq!(meta.piece_length_for(last_idx), 1000 % 256);
}

#[test]
fn test_block_length_for_middle_of_piece() {
    let data = single_file_torrent("data.bin", 32768, 16384);
    let meta = Metainfo::from_bytes(&data).unwrap();
    let bl = meta.block_length_for(0, 0);
    assert_eq!(bl, 16384);
}

#[test]
fn test_block_length_for_end_of_piece() {
    let data = single_file_torrent("data.bin", 32768, 16384);
    let meta = Metainfo::from_bytes(&data).unwrap();
    let bl = meta.block_length_for(0, 16384 - 1);
    assert_eq!(bl, 1);
}

#[test]
fn test_block_length_for_beyond_piece() {
    let data = single_file_torrent("data.bin", 32768, 16384);
    let meta = Metainfo::from_bytes(&data).unwrap();
    let bl = meta.block_length_for(0, 20000);
    assert_eq!(bl, 0);
}

#[test]
fn test_block_length_for_last_piece() {
    let data = single_file_torrent("data.bin", 20000, 16384);
    let meta = Metainfo::from_bytes(&data).unwrap();
    let last_idx = meta.piece_count() - 1;
    let bl = meta.block_length_for(last_idx, 0);
    assert_eq!(bl, 20000 - 16384);
}

#[test]
fn test_is_single_file_true() {
    let data = single_file_torrent("a.bin", 100, 256);
    let meta = Metainfo::from_bytes(&data).unwrap();
    assert!(meta.is_single_file());
    assert!(!meta.is_multi_file());
}

#[test]
fn test_is_multi_file_true() {
    let info = make_multi_file_info("dir", vec![("a", 10), ("b", 20)], 256, 1);
    let data = make_torrent_bytes(vec![(b"info".to_vec(), info)]);
    let meta = Metainfo::from_bytes(&data).unwrap();
    assert!(!meta.is_single_file());
    assert!(meta.is_multi_file());
}

#[test]
fn test_display_format() {
    let data = single_file_torrent("display_test.bin", 1048576, 262144);
    let meta = Metainfo::from_bytes(&data).unwrap();
    let s = format!("{}", meta);
    assert!(s.contains("display_test.bin"));
    assert!(s.contains("1048576 bytes"));
    assert!(s.contains("1.00 MiB"));
    assert!(s.contains("piece_count"));
}

#[test]
fn test_error_missing_info_dict() {
    let data = make_torrent_bytes(vec![
        (b"announce".to_vec(), b_str("http://t.com/ann")),
    ]);
    assert!(Metainfo::from_bytes(&data).is_err());
}

#[test]
fn test_error_missing_pieces() {
    let info = build_dict(vec![
        (b"piece length".to_vec(), b_int(256)),
        (b"name".to_vec(), b_str("x.bin")),
        (b"length".to_vec(), b_int(100)),
    ]);
    let data = make_torrent_bytes(vec![
        (b"info".to_vec(), info),
    ]);
    assert!(Metainfo::from_bytes(&data).is_err());
}

#[test]
fn test_error_invalid_pieces_length() {
    let info = build_dict(vec![
        (b"piece length".to_vec(), b_int(256)),
        (b"pieces".to_vec(), BencodeValue::ByteString(vec![0u8; 15])),
        (b"name".to_vec(), b_str("x.bin")),
        (b"length".to_vec(), b_int(100)),
    ]);
    let data = make_torrent_bytes(vec![
        (b"info".to_vec(), info),
    ]);
    assert!(Metainfo::from_bytes(&data).is_err());
}

#[test]
fn test_error_negative_piece_length() {
    let info = build_dict(vec![
        (b"piece length".to_vec(), b_int(-1)),
        (b"pieces".to_vec(), BencodeValue::ByteString(sha1_pieces(1))),
        (b"name".to_vec(), b_str("x.bin")),
        (b"length".to_vec(), b_int(100)),
    ]);
    let data = make_torrent_bytes(vec![
        (b"info".to_vec(), info),
    ]);
    assert!(Metainfo::from_bytes(&data).is_err());
}

#[test]
fn test_error_zero_piece_length() {
    let info = build_dict(vec![
        (b"piece length".to_vec(), b_int(0)),
        (b"pieces".to_vec(), BencodeValue::ByteString(sha1_pieces(1))),
        (b"name".to_vec(), b_str("x.bin")),
        (b"length".to_vec(), b_int(100)),
    ]);
    let data = make_torrent_bytes(vec![
        (b"info".to_vec(), info),
    ]);
    assert!(Metainfo::from_bytes(&data).is_err());
}

#[test]
fn test_error_missing_name() {
    let info = build_dict(vec![
        (b"piece length".to_vec(), b_int(256)),
        (b"pieces".to_vec(), BencodeValue::ByteString(sha1_pieces(1))),
        (b"length".to_vec(), b_int(100)),
    ]);
    let data = make_torrent_bytes(vec![
        (b"info".to_vec(), info),
    ]);
    assert!(Metainfo::from_bytes(&data).is_err());
}

#[test]
fn test_error_negative_file_length_single() {
    let info = build_dict(vec![
        (b"piece length".to_vec(), b_int(256)),
        (b"pieces".to_vec(), BencodeValue::ByteString(sha1_pieces(1))),
        (b"name".to_vec(), b_str("x.bin")),
        (b"length".to_vec(), b_int(-50)),
    ]);
    let data = make_torrent_bytes(vec![
        (b"info".to_vec(), info),
    ]);
    assert!(Metainfo::from_bytes(&data).is_err());
}

#[test]
fn test_error_negative_file_length_multi() {
    let file_entries = b_list(vec![
        build_dict(vec![
            (b"length".to_vec(), b_int(-100)),
            (b"path".to_vec(), b_list(vec![b_str("bad.dat")])),
        ]),
    ]);
    let info = build_dict(vec![
        (b"piece length".to_vec(), b_int(256)),
        (b"pieces".to_vec(), BencodeValue::ByteString(sha1_pieces(1))),
        (b"name".to_vec(), b_str("dir")),
        (b"files".to_vec(), file_entries),
    ]);
    let data = make_torrent_bytes(vec![
        (b"info".to_vec(), info),
    ]);
    assert!(Metainfo::from_bytes(&data).is_err());
}

#[test]
fn test_error_piece_count_mismatch_too_few() {
    let info = build_dict(vec![
        (b"piece length".to_vec(), b_int(256)),
        (b"pieces".to_vec(), BencodeValue::ByteString(sha1_pieces(1))),
        (b"name".to_vec(), b_str("x.bin")),
        (b"length".to_vec(), b_int(1024)),
    ]);
    let data = make_torrent_bytes(vec![
        (b"info".to_vec(), info),
    ]);
    assert!(Metainfo::from_bytes(&data).is_err());
}

#[test]
fn test_error_piece_count_mismatch_too_many() {
    let info = build_dict(vec![
        (b"piece length".to_vec(), b_int(256)),
        (b"pieces".to_vec(), BencodeValue::ByteString(sha1_pieces(10))),
        (b"name".to_vec(), b_str("x.bin")),
        (b"length".to_vec(), b_int(256)),
    ]);
    let data = make_torrent_bytes(vec![
        (b"info".to_vec(), info),
    ]);
    assert!(Metainfo::from_bytes(&data).is_err());
}

#[test]
fn test_error_no_files() {
    let info = build_dict(vec![
        (b"piece length".to_vec(), b_int(256)),
        (b"pieces".to_vec(), BencodeValue::ByteString(sha1_pieces(1))),
        (b"name".to_vec(), b_str("x.bin")),
    ]);
    let data = make_torrent_bytes(vec![
        (b"info".to_vec(), info),
    ]);
    assert!(Metainfo::from_bytes(&data).is_err());
}

#[test]
fn test_error_empty_files_list() {
    let info = build_dict(vec![
        (b"piece length".to_vec(), b_int(256)),
        (b"pieces".to_vec(), BencodeValue::ByteString(sha1_pieces(1))),
        (b"name".to_vec(), b_str("dir")),
        (b"files".to_vec(), b_list(vec![])),
    ]);
    let data = make_torrent_bytes(vec![
        (b"info".to_vec(), info),
    ]);
    assert!(Metainfo::from_bytes(&data).is_err());
}

#[test]
fn test_error_missing_path_in_file_entry() {
    let file_entry = build_dict(vec![
        (b"length".to_vec(), b_int(100)),
    ]);
    let info = build_dict(vec![
        (b"piece length".to_vec(), b_int(256)),
        (b"pieces".to_vec(), BencodeValue::ByteString(sha1_pieces(1))),
        (b"name".to_vec(), b_str("dir")),
        (b"files".to_vec(), b_list(vec![file_entry])),
    ]);
    let data = make_torrent_bytes(vec![
        (b"info".to_vec(), info),
    ]);
    assert!(Metainfo::from_bytes(&data).is_err());
}

#[test]
fn test_error_empty_path_in_file_entry() {
    let file_entry = build_dict(vec![
        (b"length".to_vec(), b_int(100)),
        (b"path".to_vec(), b_list(vec![])),
    ]);
    let info = build_dict(vec![
        (b"piece length".to_vec(), b_int(256)),
        (b"pieces".to_vec(), BencodeValue::ByteString(sha1_pieces(1))),
        (b"name".to_vec(), b_str("dir")),
        (b"files".to_vec(), b_list(vec![file_entry])),
    ]);
    let data = make_torrent_bytes(vec![
        (b"info".to_vec(), info),
    ]);
    assert!(Metainfo::from_bytes(&data).is_err());
}

#[test]
fn test_error_missing_length_in_file_entry() {
    let file_entry = build_dict(vec![
        (b"path".to_vec(), b_list(vec![b_str("file.dat")])),
    ]);
    let info = build_dict(vec![
        (b"piece length".to_vec(), b_int(256)),
        (b"pieces".to_vec(), BencodeValue::ByteString(sha1_pieces(1))),
        (b"name".to_vec(), b_str("dir")),
        (b"files".to_vec(), b_list(vec![file_entry])),
    ]);
    let data = make_torrent_bytes(vec![
        (b"info".to_vec(), info),
    ]);
    assert!(Metainfo::from_bytes(&data).is_err());
}

#[test]
fn test_announce_list_with_empty_inner_tier() {
    let info = make_single_file_info("x.bin", 100, 256, 1);
    let announce_list = b_list(vec![
        b_list(vec![]),
        b_list(vec![b_str("http://ok.com/ann")]),
    ]);
    let data = make_torrent_bytes(vec![
        (b"info".to_vec(), info),
        (b"announce-list".to_vec(), announce_list),
    ]);
    let meta = Metainfo::from_bytes(&data).unwrap();
    let list = meta.announce_list.as_ref().unwrap();
    assert_eq!(list.len(), 1);
}

#[test]
fn test_announce_list_all_empty_tiers() {
    let info = make_single_file_info("x.bin", 100, 256, 1);
    let announce_list = b_list(vec![
        b_list(vec![]),
        b_list(vec![]),
    ]);
    let data = make_torrent_bytes(vec![
        (b"info".to_vec(), info),
        (b"announce-list".to_vec(), announce_list),
    ]);
    let meta = Metainfo::from_bytes(&data).unwrap();
    assert_eq!(meta.announce_list, None);
}

#[test]
fn test_parse_raw_torrent_bytes() {
    let raw = b"d8:announce30:http://tracker.example.com/ann4:infod12:piece lengthi262144e6:pieces20:xxxxxxxxxxxxxxxxxxxx4:name8:test.txt6:lengthi1024eee".to_vec();
    let meta = Metainfo::from_bytes(&raw).unwrap();
    assert_eq!(meta.announce.as_deref(), Some("http://tracker.example.com/ann"));
    assert_eq!(meta.info.name, "test.txt");
    assert_eq!(meta.info.piece_length, 262144);
    assert_eq!(meta.info.total_length, 1024);
    assert_eq!(meta.piece_count(), 1);
}

#[test]
fn test_multi_file_cross_piece_boundary() {
    let info = make_multi_file_info(
        "cross",
        vec![("a.bin", 300), ("b.bin", 212)],
        256,
        2,
    );
    let data = make_torrent_bytes(vec![
        (b"info".to_vec(), info),
    ]);
    let meta = Metainfo::from_bytes(&data).unwrap();
    assert_eq!(meta.info.total_length, 512);
    assert_eq!(meta.piece_count(), 2);
    assert_eq!(meta.info.files.len(), 2);
}

#[test]
fn test_multi_file_total_length_sum() {
    let files: Vec<(&str, i64)> = vec![
        ("a.dat", 100),
        ("b.dat", 200),
        ("c.dat", 300),
        ("d.dat", 400),
        ("e.dat", 500),
    ];
    let total: i64 = files.iter().map(|(_, l)| l).sum();
    let piece_length: i64 = 512;
    let piece_count = ((total as usize + piece_length as usize - 1) / piece_length as usize).max(1);
    let info = make_multi_file_info("sum_test", files, piece_length, piece_count);
    let data = make_torrent_bytes(vec![
        (b"info".to_vec(), info),
    ]);
    let meta = Metainfo::from_bytes(&data).unwrap();
    assert_eq!(meta.info.total_length, total as usize);
}

#[test]
fn test_block_length_for_exact_block_boundary() {
    let data = single_file_torrent("data.bin", 32768, 16384);
    let meta = Metainfo::from_bytes(&data).unwrap();
    let bl = meta.block_length_for(0, 16384);
    assert_eq!(bl, 0);
}

#[test]
fn test_block_length_for_clamped_to_block_len() {
    let data = single_file_torrent("data.bin", 1048576, 262144);
    let meta = Metainfo::from_bytes(&data).unwrap();
    let bl = meta.block_length_for(0, 0);
    assert_eq!(bl, 16384);
}

#[test]
fn test_piece_length_for_single_piece_torrent() {
    let data = single_file_torrent("tiny.bin", 42, 262144);
    let meta = Metainfo::from_bytes(&data).unwrap();
    assert_eq!(meta.piece_length_for(0), 42);
}

#[test]
fn test_single_file_has_correct_path() {
    let data = single_file_torrent("hello.world.txt", 4096, 1024);
    let meta = Metainfo::from_bytes(&data).unwrap();
    assert_eq!(meta.info.files[0].path, vec!["hello.world.txt"]);
    assert_eq!(meta.info.files[0].length, 4096);
}

#[test]
fn test_info_hash_changes_with_piece_length() {
    let d1 = single_file_torrent("x.bin", 4096, 256);
    let d2 = single_file_torrent("x.bin", 4096, 512);
    assert_ne!(
        Metainfo::from_bytes(&d1).unwrap().info_hash,
        Metainfo::from_bytes(&d2).unwrap().info_hash,
    );
}

#[test]
fn test_info_hash_changes_with_name() {
    let d1 = single_file_torrent("aaa.bin", 1024, 256);
    let d2 = single_file_torrent("bbb.bin", 1024, 256);
    assert_ne!(
        Metainfo::from_bytes(&d1).unwrap().info_hash,
        Metainfo::from_bytes(&d2).unwrap().info_hash,
    );
}

#[test]
fn test_announce_list_without_announce() {
    let info = make_single_file_info("x.bin", 100, 256, 1);
    let announce_list = b_list(vec![
        b_list(vec![b_str("udp://x.com:80/ann"), b_str("http://y.com/ann")]),
    ]);
    let data = make_torrent_bytes(vec![
        (b"info".to_vec(), info),
        (b"announce-list".to_vec(), announce_list),
    ]);
    let meta = Metainfo::from_bytes(&data).unwrap();
    assert_eq!(meta.announce, None);
    assert!(meta.announce_list.is_some());
    assert_eq!(meta.all_tracker_urls().len(), 2);
}