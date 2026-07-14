use backend::core::metainfo::Metainfo;
use backend::storage::assembler::FileAssembler;
use sha1::Digest;

fn make_metainfo(piece_count: usize, file_sizes: &[usize], piece_len: usize) -> Metainfo {
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
            let filename = format!("file{}.txt", i);
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
    Metainfo::from_bytes(&bencode).unwrap()
}

#[test]
fn test_single_file_one_piece() {
    let m = make_metainfo(1, &[65536], 65536);
    let assembler = FileAssembler::new(&m, std::path::Path::new("/tmp"));
    assert_eq!(assembler.piece_map.len(), 1);
    let loc = &assembler.piece_map[0];
    assert_eq!(loc.segments.len(), 1);
    assert_eq!(loc.segments[0].file_index, 0);
    assert_eq!(loc.segments[0].file_offset, 0);
    assert_eq!(loc.segments[0].byte_offset, 0);
    assert_eq!(loc.segments[0].length, 65536);
}

#[test]
fn test_single_file_multiple_pieces() {
    let m = make_metainfo(4, &[65536], 16384);
    let assembler = FileAssembler::new(&m, std::path::Path::new("/tmp"));
    assert_eq!(assembler.piece_map.len(), 4);
    for (i, loc) in assembler.piece_map.iter().enumerate() {
        assert_eq!(loc.segments.len(), 1);
        assert_eq!(loc.segments[0].byte_offset, 0);
        assert_eq!(loc.segments[0].file_offset, (i * 16384) as u64);
        assert_eq!(loc.segments[0].file_index, 0);
    }
}

#[test]
fn test_multi_file_no_cross_boundary() {
    let m = make_metainfo(2, &[16384, 16384], 16384);
    let assembler = FileAssembler::new(&m, std::path::Path::new("/tmp"));
    assert_eq!(assembler.piece_map.len(), 2);
    assert_eq!(assembler.piece_map[0].segments.len(), 1);
    assert_eq!(assembler.piece_map[0].segments[0].file_index, 0);
    assert_eq!(assembler.piece_map[1].segments.len(), 1);
    assert_eq!(assembler.piece_map[1].segments[0].file_index, 1);
}

#[test]
fn test_cross_file_boundary() {
    let m = make_metainfo(1, &[8192, 8192], 16384);
    let assembler = FileAssembler::new(&m, std::path::Path::new("/tmp"));
    assert_eq!(assembler.piece_map.len(), 1);
    let loc = &assembler.piece_map[0];
    assert_eq!(loc.segments.len(), 2);
    assert_eq!(loc.segments[0].file_index, 0);
    assert_eq!(loc.segments[0].file_offset, 0);
    assert_eq!(loc.segments[0].byte_offset, 0);
    assert_eq!(loc.segments[0].length, 8192);
    assert_eq!(loc.segments[1].file_index, 1);
    assert_eq!(loc.segments[1].file_offset, 0);
    assert_eq!(loc.segments[1].byte_offset, 8192);
    assert_eq!(loc.segments[1].length, 8192);
}

#[test]
fn test_cross_multiple_file_boundaries() {
    let m = make_metainfo(1, &[4096, 4096, 4096, 4096], 16384);
    let assembler = FileAssembler::new(&m, std::path::Path::new("/tmp"));
    let loc = &assembler.piece_map[0];
    assert_eq!(loc.segments.len(), 4);
    for (i, seg) in loc.segments.iter().enumerate() {
        assert_eq!(seg.file_index, i);
        assert_eq!(seg.file_offset, 0);
        assert_eq!(seg.byte_offset, (i * 4096) as u64);
        assert_eq!(seg.length, 4096);
    }
}

#[test]
fn test_last_piece_shorter() {
    let m = make_metainfo(2, &[16384, 8192], 16384);
    let assembler = FileAssembler::new(&m, std::path::Path::new("/tmp"));
    assert_eq!(assembler.piece_map.len(), 2);
    assert_eq!(assembler.piece_map[1].segments.len(), 1);
    assert_eq!(assembler.piece_map[1].segments[0].length, 8192);
}
