use sha1::Digest;

use crate::core::types::{InfoHash, SHA1_LEN};
use crate::core::bencode::BencodeValue;
use crate::core::error::BError;

// TorrentFile contains the information of a single file
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TorrentFile {
    pub path: Vec<String>,
    pub length: usize
}

// InfoDict contains a .torrent file's info dictionary
#[derive(Clone, Debug)]
pub struct InfoDict {
    pub piece_length: usize,
    pub pieces: Vec<[u8; SHA1_LEN]>,
    pub name: String,
    pub files: Vec<TorrentFile>,
    pub total_length: usize,
    pub raw_info_bytes: Vec<u8>
}

// Metainfo contains a .torrent file's all meta information
#[derive(Clone, Debug)]
pub struct Metainfo {
    pub announce: Option<String>,
    pub announce_list: Option<Vec<Vec<String>>>,
    pub creation_date: Option<i64>,
    pub created_by: Option<String>,
    pub encoding: Option<String>,
    pub comment: Option<String>,
    pub info: InfoDict,
    pub info_hash: InfoHash
}

impl Metainfo {
    pub fn from_bytes(data: &[u8]) -> Result<Self, BError> {
        let (root, _) = BencodeValue::decode(data)?;

        let info_value = root
            .dict_get(b"info")
            .ok_or(BError::MissingInfoDict)?;
        let raw_info_bytes = Self::extract_raw_info(data)?;
        let info_hash = InfoHash::from_bytes(sha1::Sha1::digest(&raw_info_bytes).into());
        let info = Self::parse_info_dict(info_value, raw_info_bytes)?;
        let announce = root.dict_get_str(b"announce");
        let announce_list = Self::parse_announce_list(&root);
        let creation_date = root.dict_get_int(b"creation date");
        let created_by = root.dict_get_str(b"created by");
        let encoding = root.dict_get_str(b"encoding");
        let comment = root.dict_get_str(b"comment");

        Ok(Metainfo {
            announce,
            announce_list,
            creation_date,
            created_by,
            encoding,
            comment,
            info,
            info_hash,
        })
    }

    fn extract_raw_info(data: &[u8]) -> Result<Vec<u8>, BError> {
        let marker = b"4:info";
        let pos = data
            .windows(marker.len())
            .position(|w| w == marker)
            .ok_or(BError::InvalidTorrent(
                "cannot find 'info' key in torrent file".into()
            ))?;
        let value_start = pos + marker.len();
        let info_bytes = &data[value_start..];
        let (_, consumed) = BencodeValue::decode(info_bytes)?;
        Ok(info_bytes[..consumed].to_vec())
    }

    fn parse_info_dict(info_value: &BencodeValue, raw_bytes: Vec<u8>) -> Result<InfoDict, BError> {
        let piece_length = info_value
            .dict_get_int(b"piece length")
            .ok_or_else(|| BError::InvalidTorrent("missing 'piece length'".into()))?;
        if piece_length <= 0 {
            return Err(BError::InvalidPieceLength(piece_length));
        }
        let piece_length = piece_length as usize;
        let pieces_raw = info_value
            .dict_get_bytes(b"pieces")
            .ok_or(BError::MissingPieces)?;
        if pieces_raw.len() % SHA1_LEN != 0 {
            return Err(BError::InvalidPiecesLength(pieces_raw.len()));
        }
        let pieces: Vec<[u8; SHA1_LEN]> = pieces_raw
            .chunks_exact(SHA1_LEN)
            .map(|chunk| {
                let mut arr = [0u8; SHA1_LEN];
                arr.copy_from_slice(chunk);
                arr
            })
            .collect();
        let name = info_value
            .dict_get_str(b"name")
            .ok_or_else(|| BError::InvalidTorrent("missing 'name'".into()))?;

        let files = if let Some(length) = info_value.dict_get_int(b"length") {
            if length < 0 {
                return Err(BError::InvalidTorrent("negative file length".into()));
            }
            vec![TorrentFile {
                path: vec![name.clone()],
                length: length as usize,
            }]
        } else if let Some(files_list) = info_value.dict_get(b"files") {
            Self::parse_files_list(files_list)?
        } else {
            return Err(BError::NoFiles);
        };

        let total_length: usize = files.iter().map(|f| f.length).sum();

        if pieces.len() != Self::expected_piece_count(total_length, piece_length) {
            return Err(BError::InvalidTorrent(format!(
                "piece count mismatch: got {} pieces, expected {} for {} total bytes at {} piece_length",
                pieces.len(),
                Self::expected_piece_count(total_length, piece_length),
                total_length,
                piece_length
            )));
        }

        Ok(InfoDict {
            piece_length,
            pieces,
            name,
            files,
            total_length,
            raw_info_bytes: raw_bytes,
        })
    }

    fn parse_files_list(files_value: &BencodeValue) -> Result<Vec<TorrentFile>, BError> {
        let items = files_value
            .as_list()
            .ok_or_else(|| BError::InvalidTorrent("'files' is not a list".into()))?;

        let mut files = Vec::new();
        for item in items {
            let length = item
                .dict_get_int(b"length")
                .ok_or_else(|| BError::InvalidTorrent("file entry missing 'length'".into()))?;
            if length < 0 {
                return Err(BError::InvalidTorrent("negative file length".into()));
            }

            let path_list = item
                .dict_get(b"path")
                .and_then(|v| v.as_list())
                .ok_or_else(|| BError::InvalidTorrent("file entry missing 'path'".into()))?;

            let path: Vec<String> = path_list
                .iter()
                .map(|seg| {
                    seg.as_str()
                        .unwrap_or_else(|_| String::from("(binary path segment)"))
                })
                .collect();

            if path.is_empty() {
                return Err(BError::InvalidTorrent("file entry has empty path".into()));
            }

            files.push(TorrentFile {
                path,
                length: length as usize,
            });
        }

        if files.is_empty() {
            return Err(BError::InvalidTorrent("'files' list is empty".into()));
        }

        Ok(files)
    }

    fn parse_announce_list(root: &BencodeValue) -> Option<Vec<Vec<String>>> {
        let list = root.dict_get(b"announce-list")?;
        let outer = list.as_list()?;
        let mut result = Vec::new();
        for tier in outer {
            let inner = tier.as_list()?;
            let urls: Vec<String> = inner
                .iter()
                .filter_map(|v| v.as_str().ok())
                .collect();
            if !urls.is_empty() {
                result.push(urls);
            }
        }
        if result.is_empty() {
            None
        } else {
            Some(result)
        }
    }

    #[inline]
    fn expected_piece_count(total_length: usize, piece_length: usize) -> usize {
        ((total_length + piece_length - 1) / piece_length) as usize
    }

    pub fn piece_count(&self) -> usize {
        self.info.pieces.len()
    }

    pub fn piece_hash(&self, index: usize) -> Option<&[u8; SHA1_LEN]> {
        self.info.pieces.get(index)
    }

    pub fn last_piece_length(&self) -> usize {
        let remainder = self.info.total_length % self.info.piece_length;
        if remainder == 0 {
            self.info.piece_length
        } else {
            remainder
        }
    }

    pub fn piece_length_for(&self, index: usize) -> usize {
        if index == self.piece_count().saturating_sub(1) {
            self.last_piece_length()
        } else {
            self.info.piece_length
        }
    }

    pub fn block_length_for(&self, index: usize, begin: u32) -> u32 {
        let piece_len = self.piece_length_for(index) as u32;
        let remaining = piece_len.saturating_sub(begin);
        remaining.min(crate::core::types::BLOCK_LEN)
    }

    pub fn all_tracker_urls(&self) -> Vec<String> {
        let mut urls = Vec::new();
        if let Some(ref a) = self.announce {
            urls.push(a.clone());
        }
        if let Some(ref tiers) = self.announce_list {
            for tier in tiers {
                for url in tier {
                    if !urls.contains(url) {
                        urls.push(url.clone());
                    }
                }
            }
        }
        urls
    }

    pub fn is_single_file(&self) -> bool {
        self.info.files.len() == 1
    }

    pub fn is_multi_file(&self) -> bool {
        self.info.files.len() > 1
    }
}

impl std::fmt::Display for Metainfo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "Torrent: {}", self.info.name)?;
        writeln!(f, "  info_hash: {}", self.info_hash)?;
        writeln!(f, "  piece_length: {} bytes", self.info.piece_length)?;
        writeln!(f, "  piece_count:  {}", self.piece_count())?;
        writeln!(f, "  total_length: {} bytes ({:.2} MiB)",
            self.info.total_length,
            self.info.total_length as f64 / (1024.0 * 1024.0)
        )?;
        if let Some(ref a) = self.announce {
            writeln!(f, "  announce: {}", a)?;
        }
        writeln!(f, "  files ({}):", self.info.files.len())?;
        for file in &self.info.files {
            let path = file.path.join("/");
            writeln!(f, "    {} ({} bytes)", path, file.length)?;
        }
        Ok(())
    }
}