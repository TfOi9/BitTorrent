use crate::core::types::PieceIndex;

// Bitfield: Bitmap denoting whether a client has a piece
// every bit denotes a piece's status, 0 for no, 1 for yes
// size = ceil(total_pieces / 8)
// MSB first: byte[0]: bit7 bit6 ... bit0
//                    piece0 ... ... piece7
#[derive(Clone, PartialEq, Eq)]
pub struct Bitfield {
    bytes: Vec<u8>,
    total_pieces: usize
}

impl Bitfield {
    pub fn new(total_pieces: usize) -> Self {
        let byte_count = (total_pieces + 7) / 8;
        Self {
            bytes: vec![0u8; byte_count],
            total_pieces
        }
    }

    fn clear_trailing_bits(&mut self) {
        let trailing = self.total_pieces % 8;
        if trailing != 0 {
            let mask = 0xFFu8 << (8 - trailing);
            if let Some(last) = self.bytes.last_mut() {
                *last &= mask;
            }
        }
    }

    pub fn from_bytes(bytes: Vec<u8>, total_pieces: usize) -> Self {
        let expected = (total_pieces + 7) / 8;
        let mut bf = Self {
            bytes,
            total_pieces
        };
        bf.bytes.resize(expected, 0);
        bf.clear_trailing_bits();
        bf
    }

    #[inline]
    pub fn as_bytes(&self) -> &[u8] {
        &self.bytes
    }

    #[inline]
    pub fn total_pieces(&self) -> usize {
        self.total_pieces
    }

    pub fn has(&self, index: usize) -> bool {
        if index >= self.total_pieces {
            return false;
        }
        let byte_idx = index / 8;
        let bit_idx = 7 - (index % 8);
        self.bytes[byte_idx] & (1 << bit_idx) != 0
    }

    #[inline]
    pub fn has_piece(&self, index: PieceIndex) -> bool {
        self.has(index.as_usize())
    }

    pub fn set_piece(&mut self, index: PieceIndex) {
        self.set(index.as_usize());
    }

    pub fn set(&mut self, index: usize) {
        if index >= self.total_pieces {
            return;
        }
        let byte_idx = index / 8;
        let bit_idx = 7 - (index % 8);
        self.bytes[byte_idx] |= 1 << bit_idx;
    }

    pub fn count_complete(&self) -> usize {
        self.bytes.iter().map(|b| b.count_ones() as usize).sum()
    }

    pub fn is_complete(&self) -> bool {
        if self.total_pieces == 0 {
            return true;
        }
        self.count_complete() == self.total_pieces
    }

    pub fn missing_pieces(&self) -> impl Iterator<Item = usize> + '_ {
        (0..self.total_pieces).filter(|&i| !self.has(i))
    }

    pub fn complete_pieces(&self) -> impl Iterator<Item = usize> + '_ {
        (0..self.total_pieces).filter(|&i| self.has(i))
    }
}

impl std::fmt::Debug for Bitfield {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Bitfield({}/{})",
            self.count_complete(),
            self.total_pieces
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bitfield_new() {
        let bf = Bitfield::new(10);
        assert_eq!(bf.total_pieces(), 10);
        assert_eq!(bf.as_bytes().len(), 2); // ceil(10/8) = 2
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
}