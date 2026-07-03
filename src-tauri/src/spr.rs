// Adapted from SpriteForge's spr_manager.rs, with automatic detection of the
// sprite-count width (u16 legacy vs u32 extended) so any client version opens
// without asking the user.

use rayon::prelude::*;
use serde::Serialize;
use std::collections::HashMap;
use std::fs::File;
use std::io::{BufReader, Read, Seek, SeekFrom};
use std::sync::{Arc, Mutex};

pub const SPRITE_SIZE: usize = 32;
const SPRITE_PIXELS: usize = SPRITE_SIZE * SPRITE_SIZE;
const SPRITE_DATA_SIZE: usize = SPRITE_PIXELS * 4;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SprInfo {
    pub signature: u32,
    pub sprite_count: u32,
    pub extended: bool,
    pub file_size: u64,
}

#[derive(Debug, Clone)]
pub struct SpriteData {
    pub id: u32,
    pub is_empty: bool,
    pub compressed_pixels: Vec<u8>,
}

pub struct SprFileReader {
    file: BufReader<File>,
    info: SprInfo,
    header_size: u64,
}

/// Scores a candidate header layout by checking how many of the first address
/// table entries are plausible (zero, or pointing past the table and inside the file).
fn score_layout(reader: &mut BufReader<File>, header_size: u64, count: u32, file_len: u64) -> Option<f64> {
    if count == 0 {
        return None;
    }
    let table_end = header_size + count as u64 * 4;
    if table_end > file_len {
        return None;
    }
    let sample = count.min(2048) as usize;
    let mut buf = vec![0u8; sample * 4];
    reader.seek(SeekFrom::Start(header_size)).ok()?;
    reader.read_exact(&mut buf).ok()?;

    let mut valid = 0usize;
    for i in 0..sample {
        let addr = u32::from_le_bytes([buf[i * 4], buf[i * 4 + 1], buf[i * 4 + 2], buf[i * 4 + 3]]) as u64;
        if addr == 0 || (addr >= table_end && addr + 5 <= file_len) {
            valid += 1;
        }
    }
    Some(valid as f64 / sample as f64)
}

impl SprFileReader {
    pub fn open(path: &str, force_extended: Option<bool>) -> Result<Self, String> {
        let file = File::open(path).map_err(|e| format!("Failed to open SPR file: {}", e))?;
        let file_len = file
            .metadata()
            .map_err(|e| format!("Failed to stat SPR file: {}", e))?
            .len();

        let mut reader = BufReader::new(file);

        let mut head = [0u8; 8];
        reader
            .read_exact(&mut head)
            .map_err(|e| format!("Failed to read SPR header: {}", e))?;

        let signature = u32::from_le_bytes([head[0], head[1], head[2], head[3]]);
        let count16 = u16::from_le_bytes([head[4], head[5]]) as u32;
        let count32 = u32::from_le_bytes([head[4], head[5], head[6], head[7]]);

        let extended = match force_extended {
            Some(v) => v,
            None => {
                let s16 = score_layout(&mut reader, 6, count16, file_len);
                let s32 = score_layout(&mut reader, 8, count32, file_len);
                match (s16, s32) {
                    (Some(a), Some(b)) => b > a,
                    (Some(_), None) => false,
                    (None, Some(_)) => true,
                    (None, None) => return Err("File does not look like a valid .spr file".to_string()),
                }
            }
        };

        let (sprite_count, header_size) = if extended { (count32, 8u64) } else { (count16, 6u64) };
        if sprite_count == 0 {
            return Err("SPR file contains no sprites".to_string());
        }

        Ok(Self {
            file: reader,
            info: SprInfo {
                signature,
                sprite_count,
                extended,
                file_size: file_len,
            },
            header_size,
        })
    }

    pub fn get_info(&self) -> &SprInfo {
        &self.info
    }

    /// One byte per sprite id (1..=count): 1 if the sprite has pixel data, 0 if empty.
    pub fn read_flags(&mut self) -> Result<Vec<u8>, String> {
        let count = self.info.sprite_count as usize;
        self.file
            .seek(SeekFrom::Start(self.header_size))
            .map_err(|e| format!("Failed to seek to address table: {}", e))?;

        let mut buf = vec![0u8; count * 4];
        let mut read = 0usize;
        while read < buf.len() {
            match self.file.read(&mut buf[read..]) {
                Ok(0) => break,
                Ok(n) => read += n,
                Err(e) => return Err(format!("Failed to read address table: {}", e)),
            }
        }

        let mut flags = vec![0u8; count];
        for i in 0..count {
            let off = i * 4;
            if off + 4 <= read {
                let addr = u32::from_le_bytes([buf[off], buf[off + 1], buf[off + 2], buf[off + 3]]);
                flags[i] = (addr != 0) as u8;
            }
        }
        Ok(flags)
    }

    pub fn read_sprites_list(&mut self, ids: &[u32]) -> Result<Vec<SpriteData>, String> {
        if ids.is_empty() {
            return Ok(Vec::new());
        }

        let mut sorted_ids: Vec<u32> = ids.to_vec();
        sorted_ids.sort_unstable();
        sorted_ids.dedup();

        let max_id = self.info.sprite_count;
        let file_ids: Vec<u32> = sorted_ids.into_iter().filter(|&id| id > 0 && id <= max_id).collect();
        if file_ids.is_empty() {
            return Ok(Vec::new());
        }

        let mut sprites: Vec<SpriteData> = Vec::with_capacity(file_ids.len());

        // Split into chunks of nearby ids so each chunk's address table is one read.
        let mut chunks: Vec<Vec<u32>> = Vec::new();
        let mut current: Vec<u32> = Vec::new();
        for &id in &file_ids {
            match current.last() {
                Some(&last) if id - last >= 100 => {
                    chunks.push(std::mem::take(&mut current));
                    current.push(id);
                }
                _ => current.push(id),
            }
        }
        if !current.is_empty() {
            chunks.push(current);
        }

        for chunk in chunks {
            let start_id = chunk[0];
            let end_id = *chunk.last().unwrap();
            let count = end_id - start_id + 1;

            let start_offset = self.header_size + (start_id - 1) as u64 * 4;
            self.file
                .seek(SeekFrom::Start(start_offset))
                .map_err(|e| format!("Failed to seek to address table: {}", e))?;

            let mut addresses_buf = vec![0u8; (count * 4) as usize];
            self.file
                .read_exact(&mut addresses_buf)
                .map_err(|e| format!("Failed to read address table: {}", e))?;

            let mut valid_sprites = Vec::with_capacity(chunk.len());
            for &id in &chunk {
                let off = ((id - start_id) * 4) as usize;
                let address = u32::from_le_bytes([
                    addresses_buf[off],
                    addresses_buf[off + 1],
                    addresses_buf[off + 2],
                    addresses_buf[off + 3],
                ]);
                if address != 0 {
                    valid_sprites.push((id, address as u64));
                } else {
                    sprites.push(SpriteData {
                        id,
                        is_empty: true,
                        compressed_pixels: Vec::new(),
                    });
                }
            }

            if valid_sprites.is_empty() {
                continue;
            }

            valid_sprites.sort_by_key(|k| k.1);

            let min_pos = valid_sprites.first().unwrap().1;
            let max_pos = valid_sprites.last().unwrap().1;
            let span_size = (max_pos + 8192) - min_pos;
            let estimated_data_size = valid_sprites.len() as u64 * 500;

            if span_size < 5 * 1024 * 1024 && (estimated_data_size * 5 > span_size || valid_sprites.len() > 50) {
                // Dense chunk: read the whole span in one go and slice sprites out of it.
                self.file
                    .seek(SeekFrom::Start(min_pos))
                    .map_err(|e| format!("Failed to seek to data block: {}", e))?;

                let mut file_buf = vec![0u8; span_size as usize];
                let mut bytes_read = 0usize;
                while bytes_read < file_buf.len() {
                    match self.file.read(&mut file_buf[bytes_read..]) {
                        Ok(0) => break,
                        Ok(n) => bytes_read += n,
                        Err(e) => return Err(format!("Failed to read data block: {}", e)),
                    }
                }

                for (id, pos) in valid_sprites {
                    let local_offset = (pos - min_pos) as usize;
                    if local_offset + 5 > bytes_read {
                        continue;
                    }
                    let len_offset = local_offset + 3;
                    let length = u16::from_le_bytes([file_buf[len_offset], file_buf[len_offset + 1]]);
                    if length == 0 {
                        sprites.push(SpriteData {
                            id,
                            is_empty: true,
                            compressed_pixels: Vec::new(),
                        });
                        continue;
                    }
                    let data_offset = len_offset + 2;
                    let data_end = data_offset + length as usize;
                    if data_end <= bytes_read {
                        sprites.push(SpriteData {
                            id,
                            is_empty: false,
                            compressed_pixels: file_buf[data_offset..data_end].to_vec(),
                        });
                    }
                }
            } else {
                // Sparse chunk: seek to each sprite individually.
                for (id, pos) in valid_sprites {
                    self.file
                        .seek(SeekFrom::Start(pos + 3))
                        .map_err(|e| format!("Failed to seek: {}", e))?;

                    let mut len_buf = [0u8; 2];
                    self.file
                        .read_exact(&mut len_buf)
                        .map_err(|e| format!("Failed to read length: {}", e))?;
                    let length = u16::from_le_bytes(len_buf);

                    if length == 0 {
                        sprites.push(SpriteData {
                            id,
                            is_empty: true,
                            compressed_pixels: Vec::new(),
                        });
                        continue;
                    }

                    let mut pixels = vec![0u8; length as usize];
                    self.file
                        .read_exact(&mut pixels)
                        .map_err(|e| format!("Failed to read pixels: {}", e))?;

                    sprites.push(SpriteData {
                        id,
                        is_empty: false,
                        compressed_pixels: pixels,
                    });
                }
            }
        }

        Ok(sprites)
    }
}

pub fn decompress_to_rgba(compressed: &[u8], transparent: bool) -> Vec<u8> {
    let mut pixels = vec![0u8; SPRITE_DATA_SIZE];
    let mut write_pos = 0;
    let mut read_pos = 0;
    let channels = if transparent { 4 } else { 3 };

    while read_pos + 4 <= compressed.len() && write_pos < SPRITE_DATA_SIZE {
        let transparent_count = u16::from_le_bytes([compressed[read_pos], compressed[read_pos + 1]]) as usize;
        read_pos += 2;
        let colored_count = u16::from_le_bytes([compressed[read_pos], compressed[read_pos + 1]]) as usize;
        read_pos += 2;

        let mut current_channels = channels;
        let bytes_needed = colored_count * current_channels;

        if read_pos + bytes_needed > compressed.len() {
            if transparent && read_pos + colored_count * 3 <= compressed.len() {
                current_channels = 3;
            } else {
                break;
            }
        }

        write_pos += transparent_count * 4;
        if write_pos > SPRITE_DATA_SIZE {
            break;
        }

        for _ in 0..colored_count {
            if write_pos >= SPRITE_DATA_SIZE {
                break;
            }
            let red = compressed[read_pos];
            let green = compressed[read_pos + 1];
            let blue = compressed[read_pos + 2];
            read_pos += 3;

            let alpha = if current_channels == 4 {
                let a = compressed[read_pos];
                read_pos += 1;
                a
            } else {
                0xFF
            };

            pixels[write_pos] = red;
            pixels[write_pos + 1] = green;
            pixels[write_pos + 2] = blue;
            pixels[write_pos + 3] = alpha;
            write_pos += 4;
        }
    }

    pixels
}

pub struct SprManager {
    readers: HashMap<String, SprFileReader>,
}

impl SprManager {
    pub fn new() -> Self {
        Self {
            readers: HashMap::new(),
        }
    }

    pub fn open_file(&mut self, path: String, force_extended: Option<bool>) -> Result<SprInfo, String> {
        let reader = SprFileReader::open(&path, force_extended)?;
        let info = reader.get_info().clone();
        self.readers.insert(path, reader);
        Ok(info)
    }

    pub fn close_file(&mut self, path: &str) {
        self.readers.remove(path);
    }

    fn reader(&mut self, path: &str) -> Result<&mut SprFileReader, String> {
        self.readers
            .get_mut(path)
            .ok_or_else(|| format!("SPR file not open: {}", path))
    }

    pub fn read_flags(&mut self, path: &str) -> Result<Vec<u8>, String> {
        self.reader(path)?.read_flags()
    }

    pub fn read_sprites_raw(&mut self, path: &str, ids: &[u32]) -> Result<Vec<SpriteData>, String> {
        self.reader(path)?.read_sprites_list(ids)
    }

    /// Composes a spritesheet PNG: sprites laid out left-to-right, top-to-bottom
    /// in the order given by `ids`.
    pub fn compose_atlas_png(&mut self, path: &str, ids: &[u32], cols: u32, transparent: bool) -> Result<Vec<u8>, String> {
        use std::io::Cursor;

        let cols = cols.max(1);
        let n = ids.len() as u32;
        let rows = ((n + cols - 1) / cols).max(1);
        let atlas_w = cols * SPRITE_SIZE as u32;
        let atlas_h = rows * SPRITE_SIZE as u32;

        let sprites = self.reader(path)?.read_sprites_list(ids)?;

        let decoded: HashMap<u32, Vec<u8>> = sprites
            .into_par_iter()
            .filter(|s| !s.is_empty)
            .map(|s| (s.id, decompress_to_rgba(&s.compressed_pixels, transparent)))
            .collect();

        let row_bytes = SPRITE_SIZE * 4;
        let mut atlas = vec![0u8; (atlas_w * atlas_h * 4) as usize];

        for (idx, id) in ids.iter().enumerate() {
            let Some(rgba) = decoded.get(id) else { continue };
            let dst_x = (idx as u32 % cols) * SPRITE_SIZE as u32;
            let dst_y = (idx as u32 / cols) * SPRITE_SIZE as u32;
            for y in 0..SPRITE_SIZE as u32 {
                let src_off = y as usize * row_bytes;
                let dst_off = (((dst_y + y) * atlas_w + dst_x) as usize) * 4;
                atlas[dst_off..dst_off + row_bytes].copy_from_slice(&rgba[src_off..src_off + row_bytes]);
            }
        }

        let img = image::RgbaImage::from_raw(atlas_w, atlas_h, atlas)
            .ok_or_else(|| "Failed to build atlas image".to_string())?;
        let mut out = Cursor::new(Vec::new());
        image::DynamicImage::ImageRgba8(img)
            .write_to(&mut out, image::ImageOutputFormat::Png)
            .map_err(|e| format!("PNG encode failed: {}", e))?;
        Ok(out.into_inner())
    }
}

pub type SprManagerState = Arc<Mutex<SprManager>>;
