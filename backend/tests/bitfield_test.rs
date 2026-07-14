use backend::core::bitfield::Bitfield;
use backend::core::types::PieceIndex;

#[test]
fn test_bitfield_new() {
    let bf = Bitfield::new(10);
    assert_eq!(bf.total_pieces(), 10);
    assert_eq!(bf.as_bytes().len(), 2);
    assert_eq!(bf.count_complete(), 0);
    assert!(!bf.is_complete());
}

#[test]
fn test_bitfield_set_and_check() {
    let mut bf = Bitfield::new(16);
    bf.set(0);
    bf.set(7);
    bf.set(15);
    assert!(bf.has(0));
    assert!(bf.has(7));
    assert!(bf.has(15));
    assert!(!bf.has(1));
    assert!(!bf.has(8));
    assert_eq!(bf.count_complete(), 3);
}

#[test]
fn test_bitfield_from_bytes_with_trailing() {
    let bytes = vec![0xFF, 0xFF];
    let bf = Bitfield::from_bytes(bytes, 10);
    assert_eq!(bf.count_complete(), 10);
    assert!(bf.is_complete());
    assert_eq!(bf.as_bytes()[1], 0xC0);
}

#[test]
fn test_missing_pieces() {
    let mut bf = Bitfield::new(5);
    bf.set(1);
    bf.set(3);
    let missing: Vec<usize> = bf.missing_pieces().collect();
    assert_eq!(missing, vec![0, 2, 4]);
}

#[test]
fn test_bitfield_new_zero_pieces() {
    let bf = Bitfield::new(0);
    assert_eq!(bf.total_pieces(), 0);
    assert_eq!(bf.count_complete(), 0);
    assert!(bf.is_complete());
}

#[test]
fn test_bitfield_new_one_piece() {
    let bf = Bitfield::new(1);
    assert_eq!(bf.total_pieces(), 1);
    assert_eq!(bf.as_bytes().len(), 1);
    assert!(!bf.is_complete());
}

#[test]
fn test_bitfield_new_exact_byte_boundary() {
    let bf = Bitfield::new(8);
    assert_eq!(bf.as_bytes().len(), 1);
    let bf = Bitfield::new(16);
    assert_eq!(bf.as_bytes().len(), 2);
    let bf = Bitfield::new(24);
    assert_eq!(bf.as_bytes().len(), 3);
}

#[test]
fn test_bitfield_set_out_of_bounds() {
    let mut bf = Bitfield::new(10);
    bf.set(10);
    bf.set(100);
    assert_eq!(bf.count_complete(), 0);
}

#[test]
fn test_bitfield_has_out_of_bounds() {
    let bf = Bitfield::new(10);
    assert!(!bf.has(10));
    assert!(!bf.has(100));
}

#[test]
fn test_bitfield_set_all() {
    let mut bf = Bitfield::new(20);
    for i in 0..20 {
        bf.set(i);
    }
    assert_eq!(bf.count_complete(), 20);
    assert!(bf.is_complete());
    assert!(bf.missing_pieces().next().is_none());
}

#[test]
fn test_bitfield_from_bytes_empty() {
    let bf = Bitfield::from_bytes(vec![], 0);
    assert_eq!(bf.total_pieces(), 0);
    assert!(bf.is_complete());
}

#[test]
fn test_bitfield_from_bytes_exact() {
    let bf = Bitfield::from_bytes(vec![0b10101010], 8);
    assert_eq!(bf.total_pieces(), 8);
    assert!(bf.has(0));
    assert!(!bf.has(1));
    assert!(bf.has(2));
    assert!(!bf.has(3));
    assert!(bf.has(4));
    assert!(!bf.has(5));
    assert!(bf.has(6));
    assert!(!bf.has(7));
}

#[test]
fn test_bitfield_from_bytes_truncates_excess() {
    let bf = Bitfield::from_bytes(vec![0xFF, 0xFF, 0xFF], 10);
    assert_eq!(bf.as_bytes().len(), 2);
    assert_eq!(bf.count_complete(), 10);
}

#[test]
fn test_bitfield_from_bytes_pads_short() {
    let bf = Bitfield::from_bytes(vec![0xFF], 20);
    assert_eq!(bf.as_bytes().len(), 3);
}

#[test]
fn test_bitfield_count_complete_zero() {
    let bf = Bitfield::new(100);
    assert_eq!(bf.count_complete(), 0);
}

#[test]
fn test_bitfield_is_complete_empty() {
    let bf = Bitfield::new(0);
    assert!(bf.is_complete());
}

#[test]
fn test_bitfield_is_complete_false_partial() {
    let mut bf = Bitfield::new(10);
    bf.set(0);
    assert!(!bf.is_complete());
}

#[test]
fn test_bitfield_complete_pieces() {
    let mut bf = Bitfield::new(10);
    bf.set(2);
    bf.set(5);
    bf.set(9);
    let complete: Vec<usize> = bf.complete_pieces().collect();
    assert_eq!(complete, vec![2, 5, 9]);
}

#[test]
fn test_bitfield_complete_pieces_none() {
    let bf = Bitfield::new(10);
    let complete: Vec<usize> = bf.complete_pieces().collect();
    assert!(complete.is_empty());
}

#[test]
fn test_bitfield_missing_pieces_all() {
    let bf = Bitfield::new(5);
    let missing: Vec<usize> = bf.missing_pieces().collect();
    assert_eq!(missing, vec![0, 1, 2, 3, 4]);
}

#[test]
fn test_bitfield_missing_pieces_none() {
    let mut bf = Bitfield::new(5);
    for i in 0..5 {
        bf.set(i);
    }
    let missing: Vec<usize> = bf.missing_pieces().collect();
    assert!(missing.is_empty());
}

#[test]
fn test_bitfield_has_piece() {
    let mut bf = Bitfield::new(10);
    bf.set_piece(PieceIndex(3));
    assert!(bf.has_piece(PieceIndex(3)));
    assert!(!bf.has_piece(PieceIndex(4)));
}

#[test]
fn test_bitfield_set_piece() {
    let mut bf = Bitfield::new(10);
    bf.set_piece(PieceIndex(7));
    assert!(bf.has(7));
}

#[test]
fn test_bitfield_debug() {
    let mut bf = Bitfield::new(20);
    bf.set(1);
    bf.set(19);
    let s = format!("{:?}", bf);
    assert!(s.contains("Bitfield(2/20)"));
}

#[test]
fn test_bitfield_clone() {
    let mut bf = Bitfield::new(10);
    bf.set(3);
    let cloned = bf.clone();
    assert_eq!(bf, cloned);
    assert!(cloned.has(3));
}

#[test]
fn test_bitfield_large() {
    let total = 1000;
    let mut bf = Bitfield::new(total);
    for i in (0..total).step_by(2) {
        bf.set(i);
    }
    assert_eq!(bf.count_complete(), total / 2);
    assert!(!bf.is_complete());
    for i in (1..total).step_by(2) {
        bf.set(i);
    }
    assert!(bf.is_complete());
}

#[test]
fn test_bitfield_toggle_bits() {
    let mut bf = Bitfield::new(8);
    bf.set(0);
    assert!(bf.has(0));
    bf.set(0);
    assert!(bf.has(0));
}

#[test]
fn test_bitfield_trailing_bits_single_byte_multi() {
    for total_pieces in 0..=8 {
        let mut bf = Bitfield::new(total_pieces);
        for i in 0..total_pieces {
            bf.set(i);
        }
        assert_eq!(bf.count_complete(), total_pieces);
        assert!(bf.is_complete());
    }
}
