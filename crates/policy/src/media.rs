use std::path::Path;

use crate::error::{PolicyError, PolicyResult};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MediaExpectation {
    pub path: &'static str,
    pub minimum_bytes: u64,
    pub expected_dimensions: Option<(u32, u32)>,
}

impl MediaExpectation {
    pub fn screenshot(path: &'static str) -> Self {
        Self {
            path,
            minimum_bytes: 20_000,
            expected_dimensions: Some((1440, 900)),
        }
    }

    pub fn animation(path: &'static str) -> Self {
        Self {
            path,
            minimum_bytes: 60_000,
            expected_dimensions: Some((0, 0)),
        }
    }

    pub fn video(path: &'static str) -> Self {
        Self {
            path,
            minimum_bytes: 100_000,
            expected_dimensions: None,
        }
    }
}

pub fn inspect_png(path: &Path) -> PolicyResult<Option<(u32, u32)>> {
    let bytes = read_prefix(path, 33)?;
    if bytes.len() < 24 {
        return Ok(None);
    }
    const SIGNATURE: [u8; 8] = [137, 80, 78, 71, 13, 10, 26, 10];
    if bytes[..8] != SIGNATURE {
        return Ok(None);
    }
    if &bytes[12..16] != b"IHDR" {
        return Ok(None);
    }
    let width = u32::from_be_bytes([bytes[16], bytes[17], bytes[18], bytes[19]]);
    let height = u32::from_be_bytes([bytes[20], bytes[21], bytes[22], bytes[23]]);
    Ok(Some((width, height)))
}

pub fn inspect_gif(path: &Path) -> PolicyResult<Option<(u32, u32)>> {
    let bytes = read_prefix(path, 10)?;
    if bytes.len() < 10 {
        return Ok(None);
    }
    if &bytes[..3] != b"GIF" {
        return Ok(None);
    }
    let width = u16::from_le_bytes([bytes[6], bytes[7]]) as u32;
    let height = u16::from_le_bytes([bytes[8], bytes[9]]) as u32;
    Ok(Some((width, height)))
}

pub fn inspect_webm(path: &Path) -> PolicyResult<bool> {
    let bytes = read_prefix(path, 4)?;
    Ok(bytes.len() == 4 && bytes == [0x1a, 0x45, 0xdf, 0xa3])
}

pub fn inspect_mp4(path: &Path) -> PolicyResult<bool> {
    let bytes = read_prefix(path, 12)?;
    Ok(bytes.len() == 12 && &bytes[4..8] == b"ftyp")
}

fn read_prefix(path: &Path, length: usize) -> PolicyResult<Vec<u8>> {
    use std::io::Read;
    let mut file = std::fs::File::open(path).map_err(|error| PolicyError::FileUnreadable {
        path: path.display().to_string(),
        detail: error.to_string(),
    })?;
    let mut buffer = vec![0u8; length];
    let read = file
        .read(&mut buffer)
        .map_err(|error| PolicyError::FileUnreadable {
            path: path.display().to_string(),
            detail: error.to_string(),
        })?;
    buffer.truncate(read);
    Ok(buffer)
}
