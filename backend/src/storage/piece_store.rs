use std::collections::HashSet;
use std::fs::{create_dir_all, File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

use sha1::Digest;

use crate::core::error::{BError, Result};
use crate::core::metainfo::Metainfo;
use crate::core::types::SHA1_LEN;
use crate::storage::assembler::FileAssembler;

pub struct PieceStore {
    metainfo: Metainfo,
    assembler: FileAssembler,
    output_dir: PathBuf,
    written: HashSet<u32>,
}

impl PieceStore {
    pub fn new(metainfo: Metainfo, output_dir: &Path) -> Result<Self> {
        let assembler = FileAssembler::new(&metainfo, output_dir);
        create_output_files(&assembler, output_dir, true)?;

        Ok(Self {
            metainfo,
            assembler,
            output_dir: output_dir.to_path_buf(),
            written: HashSet::new(),
        })
    }

    pub fn open_existing(metainfo: Metainfo, output_dir: &Path) -> Result<Self> {
        let assembler = FileAssembler::new(&metainfo, output_dir);
        create_output_files(&assembler, output_dir, false)?;

        Ok(Self {
            metainfo,
            assembler,
            output_dir: output_dir.to_path_buf(),
            written: HashSet::new(),
        })
    }

    pub fn write_piece(&mut self, index: u32, data: &[u8]) -> Result<()> {
        let expected = self
            .metainfo
            .piece_hash(index as usize)
            .ok_or_else(|| BError::Session(format!("no hash for piece {}", index)))?;

        let actual: [u8; SHA1_LEN] = sha1::Sha1::digest(data).into();
        if actual != *expected {
            return Err(BError::Session(format!(
                "SHA1 mismatch for piece {}",
                index
            )));
        }

        let location = &self.assembler.piece_map[index as usize];
        let file_paths = self.assembler.file_paths(&self.output_dir);

        for segment in &location.segments {
            let path = &file_paths[segment.file_index];
            let mut file = OpenOptions::new()
                .write(true)
                .create(true)
                .open(path)
                .map_err(|e| BError::Io(e))?;

            file.seek(SeekFrom::Start(segment.file_offset))
                .map_err(|e| BError::Io(e))?;

            let piece_slice =
                &data[segment.byte_offset as usize..(segment.byte_offset + segment.length) as usize];
            file.write_all(piece_slice).map_err(|e| BError::Io(e))?;
        }

        self.written.insert(index);
        Ok(())
    }

    pub fn read_piece(&self, index: u32) -> Result<Vec<u8>> {
        let piece_len = self.metainfo.piece_length_for(index as usize);
        let mut data = vec![0u8; piece_len];
        let location = &self.assembler.piece_map[index as usize];
        let file_paths = self.assembler.file_paths(&self.output_dir);

        for segment in &location.segments {
            let path = &file_paths[segment.file_index];
            let mut file = File::open(path).map_err(|e| BError::Io(e))?;

            file.seek(SeekFrom::Start(segment.file_offset))
                .map_err(|e| BError::Io(e))?;

            let dest =
                &mut data[segment.byte_offset as usize..(segment.byte_offset + segment.length) as usize];
            file.read_exact(dest).map_err(|e| BError::Io(e))?;
        }

        Ok(data)
    }

    pub fn read_block(&self, index: u32, begin: u32, length: u32) -> Result<Vec<u8>> {
        let piece = self.read_piece(index)?;
        let offset = begin as usize;
        let end = (offset + length as usize).min(piece.len());
        Ok(piece[offset..end].to_vec())
    }

    pub fn has_piece(&self, index: u32) -> bool {
        self.written.contains(&index)
    }

    pub fn preallocate(&mut self) -> Result<()> {
        let file_paths = self.assembler.file_paths(&self.output_dir);
        for (i, path) in file_paths.iter().enumerate() {
            let file_info = &self.assembler.files[i];
            let file = File::create(path).map_err(|e| BError::Io(e))?;
            file.set_len(file_info.length).map_err(|e| BError::Io(e))?;
        }
        Ok(())
    }

    pub fn metainfo(&self) -> &Metainfo {
        &self.metainfo
    }

    pub fn file_paths(&self) -> Vec<PathBuf> {
        self.assembler.file_paths(&self.output_dir)
    }
}

fn create_output_files(assembler: &FileAssembler, output_dir: &Path, create_files: bool) -> Result<()> {
    let file_paths = assembler.file_paths(output_dir);
    for path in &file_paths {
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                create_dir_all(parent).map_err(|e| BError::Io(e))?;
            }
        }
        if create_files {
            File::create(path).map_err(|e| BError::Io(e))?;
        }
    }
    Ok(())
}
