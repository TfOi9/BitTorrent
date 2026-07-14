use crate::core::metainfo::Metainfo;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileSegment {
    pub file_index: usize,
    pub file_offset: u64,
    pub byte_offset: u64,
    pub length: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PieceLocation {
    pub global_offset: u64,
    pub segments: Vec<FileSegment>,
}

pub struct FileAssembler {
    pub piece_map: Vec<PieceLocation>,
    pub files: Vec<PieceFileInfo>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PieceFileInfo {
    pub path_components: Vec<String>,
    pub length: u64,
}

impl FileAssembler {
    pub fn new(metainfo: &Metainfo, _output_dir: &std::path::Path) -> Self {
        let files: Vec<PieceFileInfo> = metainfo
            .info
            .files
            .iter()
            .map(|f| {
                let mut path_components = f.path.clone();
                if metainfo.is_single_file() {
                    path_components = vec![f.path.join("/")];
                }
                PieceFileInfo {
                    path_components,
                    length: f.length as u64,
                }
            })
            .collect();

        let piece_map = build_piece_map(metainfo, &files);
        Self { piece_map, files }
    }

    pub fn is_single_file(&self) -> bool {
        self.files.len() == 1
    }

    pub fn file_paths(&self, output_dir: &std::path::Path) -> Vec<std::path::PathBuf> {
        if self.is_single_file() {
            let path = output_dir.join(&self.files[0].path_components[0]);
            vec![path]
        } else {
            self.files
                .iter()
                .map(|f| {
                    let mut p = output_dir.to_path_buf();
                    for comp in &f.path_components {
                        p.push(comp);
                    }
                    p
                })
                .collect()
        }
    }
}

fn build_piece_map(metainfo: &Metainfo, files: &[PieceFileInfo]) -> Vec<PieceLocation> {
    let piece_count = metainfo.piece_count();
    let piece_length = metainfo.info.piece_length as u64;
    let total_length = metainfo.info.total_length as u64;
    let mut piece_map = Vec::with_capacity(piece_count);
    let mut cumulative_offsets: Vec<u64> = Vec::with_capacity(files.len());
    let mut running = 0u64;
    for f in files {
        cumulative_offsets.push(running);
        running += f.length;
    }

    for piece_index in 0..piece_count {
        let piece_start = piece_index as u64 * piece_length;
        let piece_end = (piece_start + piece_length).min(total_length);
        let mut remaining = piece_end - piece_start;
        let mut segments = Vec::new();
        let mut piece_byte_offset = 0u64;
        let mut cursor = piece_start;

        while remaining > 0 {
            let file_index = match cumulative_offsets.binary_search(&cursor) {
                Ok(i) => i,
                Err(0) => 0,
                Err(i) => i - 1,
            };

            let file_offset = cursor - cumulative_offsets[file_index];
            let available = files[file_index].length - file_offset;
            let take = available.min(remaining);

            segments.push(FileSegment {
                file_index,
                file_offset,
                byte_offset: piece_byte_offset,
                length: take,
            });

            cursor += take;
            piece_byte_offset += take;
            remaining -= take;
        }

        piece_map.push(PieceLocation {
            global_offset: piece_start,
            segments,
        });
    }

    piece_map
}
