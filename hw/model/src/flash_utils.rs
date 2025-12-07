use std::fs::File;
use std::fs::OpenOptions;
use std::io::{Result as IoResult, Seek, SeekFrom, Write};
use std::path::PathBuf;

pub const PAGE_SIZE: usize = 256;
pub const NUM_PAGES: usize = (64 * 1024 * 1024) / PAGE_SIZE; //64MB flash

/// Enum for mailbox flash operations.
#[derive(Debug, Copy, Clone, PartialEq)]
pub enum FlashOp {
    Read = 1,
    Write,
    Erase,
    Unknown,
}

impl From<u32> for FlashOp {
    fn from(cmd: u32) -> Self {
        match cmd {
            1 => FlashOp::Read,
            2 => FlashOp::Write,
            3 => FlashOp::Erase,
            _ => FlashOp::Unknown,
        }
    }
}

/// Create or initialize a flash file, given an optional path, capacity, and initial content.
/// If path is None, uses "imaginary_flash.bin" as default.
pub fn create_and_init_flash_file(
    path: Option<PathBuf>,
    capacity: usize,
    initial_content: Option<&[u8]>,
) -> IoResult<File> {
    let path = path.unwrap_or_else(|| PathBuf::from("imaginary_flash.bin"));
    let mut file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .open(&path)?;
    let metadata = file.metadata()?;
    if metadata.len() < capacity as u64 || initial_content.is_some() {
        file.set_len(capacity as u64)?;
        file.seek(SeekFrom::Start(0))?;
        // Inline logic from initialize_flash_file
        let mut remaining = capacity;
        if let Some(content) = initial_content {
            let write_size = std::cmp::min(capacity, content.len());
            file.write_all(&content[..write_size])?;
            remaining -= write_size;
        }
        let chunk = vec![0xff; 1048576]; // 1MB chunk
        while remaining > 0 {
            let write_size = std::cmp::min(remaining, chunk.len());
            file.write_all(&chunk[..write_size])?;
            remaining -= write_size;
        }
    }
    Ok(file)
}
