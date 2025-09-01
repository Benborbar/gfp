use crate::error::PakError;
use crate::pak_reader::PakReader;
use crate::utils::file_reader::VecCursor;
use crate::utils::{read_file_at, utf16le_to_utf8_inplace, xor_each_byte, zlib_decompress};
use std::ffi::CString;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom, Write};

/// total size: 45 Bytes
#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
struct RawPakInfo {
    encrypted: u8, // 1B
    /// unused
    magic: u32, // 4B
    version: u32,  // 4B
    /// unused
    hash: [u8; 20], // 20B
    /// unused
    index_size: u64, // 8B
    index_offset: u64, // 8B
}
impl RawPakInfo {
    fn is_encrypted(&self) -> bool {
        self.encrypted != 0
    }
}

#[derive(Debug, Clone, Copy)]
struct CompressionBlock {
    start: u64,
    end: u64,
}
impl CompressionBlock {
    fn offset(&self) -> u64 {
        self.start
    }
    fn size(&self) -> u64 {
        self.end - self.start
    }
}

#[derive(Debug, Clone)]
struct Entry {
    pub file_hash: [u8; 20],
    pub file_offset: u64,
    pub file_size: u64,
    pub compression_method: u32,
    pub compressed_length: u64,
    pub dummy: [u8; 21],
    pub num_of_blocks: u32,
    pub blocks: Vec<CompressionBlock>,
    pub compressed_block_size: u32,
    pub encrypted: u8,
}

/// 参考 `src/c/gfp.c`
pub struct GfpPakReaderV10 {
    pub file: File,

    is_info_loaded: bool,
    is_entries_loaded: bool,
    is_entry_paths_loaded: bool,

    // Stage: info
    info: RawPakInfo,

    // Stage: entries
    index_data: Vec<u8>,
    index_offset: usize,
    mount_point: String,
    entries: Vec<Entry>,

    // Stage entry paths
    entry_paths: Vec<String>,
}

impl GfpPakReaderV10 {
    const PAK_INFO_SIZE: usize = size_of::<RawPakInfo>();
    const OFFSET_XOR_KEY: u64 = 0xD74AF37FAA6B020Du64;
    const ENCRYPTED_XOR_KEY: u8 = 0x6Cu8;
    const DECRYPT_KEY: u8 = 0x79u8;
    const CHUNK_SIZE: usize = 65536;

    fn load_pak_info(&mut self) -> Result<(), PakError> {
        if self.is_info_loaded {
            return Ok(());
        }
        let file_size = self
            .file
            .seek(SeekFrom::End(0))
            .expect("Unable to seek to end of file");

        self.file
            .seek(SeekFrom::Start(file_size - 45))
            .expect("Unable to seek to pak header");

        let mut buffer = [0u8; Self::PAK_INFO_SIZE];
        self.file
            .read_exact(&mut buffer)
            .expect("Failed to read pak header");

        self.info = unsafe { std::mem::transmute(buffer) };

        // deobfuscation
        self.info.encrypted ^= Self::ENCRYPTED_XOR_KEY;
        self.info.index_offset ^= Self::OFFSET_XOR_KEY;

        {
            let index_size = self
                .file
                .seek(SeekFrom::End(-(self.info.index_offset as i64)))?
                - 45;
            if index_size > 52428800 {
                return Err(PakError::invalid_data(format!(
                    "Invalid index data size: {}",
                    index_size
                )));
            }
            self.info.index_size = index_size;
        }

        self.is_info_loaded = true;
        Ok(())
    }

    fn load_entries(&mut self) -> Result<(), PakError> {
        if self.is_entries_loaded {
            return Ok(());
        }

        self.load_pak_info()?;

        // Index data
        {
            let mut index_data: Vec<u8> = vec![0u8; self.info.index_size as usize];
            read_file_at(&mut self.file, &mut index_data, self.info.index_offset)?;

            if self.info.is_encrypted() {
                xor_each_byte(&mut index_data, Self::DECRYPT_KEY);
            }

            self.index_data = index_data;
        }

        // Entries
        {
            let mut index_cursor = VecCursor::new(&self.index_data);

            let mount_point_length = u32::from_le_bytes(*index_cursor.read_nocheck::<4>()) as usize;
            index_cursor.move_by(9);
            let mount_point_data = index_cursor.read_dyn(mount_point_length - 9)?;

            let entry_count = i32::from_le_bytes(*index_cursor.read_nocheck::<4>());

            self.entries = vec![
                Entry {
                    file_hash: [0; 20],
                    file_offset: 0,
                    file_size: 0,
                    compression_method: 0,
                    compressed_length: 0,
                    dummy: [0; 21],
                    num_of_blocks: 0,
                    blocks: vec![],
                    compressed_block_size: 0,
                    encrypted: 0,
                };
                entry_count as usize
            ];

            for entry_id in 0..entry_count as usize {
                let entry = &mut self.entries[entry_id];

                entry.file_hash.copy_from_slice(index_cursor.read::<20>()?);
                entry.file_offset = u64::from_le_bytes(*index_cursor.read::<8>()?);
                entry.file_size = u64::from_le_bytes(*index_cursor.read::<8>()?);
                entry.compression_method = u32::from_le_bytes(*index_cursor.read::<4>()?);
                entry.compressed_length = u64::from_le_bytes(*index_cursor.read::<8>()?);
                entry.dummy.copy_from_slice(index_cursor.read::<21>()?);

                if entry.compression_method != 0 {
                    entry.num_of_blocks = u32::from_le_bytes(*index_cursor.read::<4>()?);
                    for _ in 0..entry.num_of_blocks {
                        let block = CompressionBlock {
                            start: u64::from_le_bytes(*index_cursor.read::<8>()?),
                            end: u64::from_le_bytes(*index_cursor.read::<8>()?),
                        };
                        entry.blocks.push(block);
                    }
                } else {
                    entry.num_of_blocks = 0;
                }

                entry.compressed_block_size = u32::from_le_bytes(*index_cursor.read::<4>()?);
                entry.encrypted = index_cursor.read::<1>()?[0];
            }

            self.mount_point = CString::from_vec_with_nul(mount_point_data)?.into_string()?;
            self.index_offset = index_cursor.offset;
            self.is_entries_loaded = true;
        }
        Ok(())
    }

    fn load_entry_paths(&mut self) -> Result<(), PakError> {
        if self.is_entry_paths_loaded {
            return Ok(());
        }
        self.load_entries()?;

        let mut index_cursor = VecCursor::new_with_offset(&self.index_data, self.index_offset);

        let entry_count: u64 = u64::from_le_bytes(*index_cursor.read::<8>()?);
        let dir_count: u64 = u64::from_le_bytes(*index_cursor.read::<8>()?);

        self.entry_paths = vec![String::new(); entry_count as usize];

        for _ in 0..dir_count {
            let dir_len: usize = u32::from_le_bytes(*index_cursor.read::<4>()?) as usize;

            let dir_name =
                CString::from_vec_with_nul(index_cursor.read_dyn(dir_len)?)?.into_string()?;

            let dir_files = u64::from_le_bytes(*index_cursor.read::<8>()?);
            for _ in 0..dir_files {
                let entry_path_size: i32 = i32::from_le_bytes(*index_cursor.read::<4>()?);
                let entry_path = if entry_path_size > 0 {
                    let data = index_cursor.read_dyn(entry_path_size as usize)?;
                    CString::from_vec_with_nul(data)?.into_string()?
                } else {
                    let mut data = index_cursor.read_dyn((-entry_path_size * 2) as usize)?;
                    utf16le_to_utf8_inplace(&mut data);
                    CString::from_vec_with_nul(data)?.into_string()?
                };

                let entry_id = i32::from_le_bytes(*index_cursor.read::<4>()?);
                if entry_id < 0 {
                    return Err(PakError::invalid_data(format!(
                        "Negative entry_id: {}",
                        entry_id
                    )));
                }
                self.entry_paths[entry_id as usize] =
                    format!("{}{}{}", self.mount_point, dir_name, entry_path);
            }
        }
        self.is_entry_paths_loaded = true;
        Ok(())
    }
}

impl PakReader for GfpPakReaderV10 {
    fn new(file: File) -> Self {
        Self {
            file,
            is_info_loaded: false,
            is_entries_loaded: false,
            is_entry_paths_loaded: false,

            info: RawPakInfo {
                encrypted: 0,
                magic: 0,
                version: 0,
                hash: [0; 20],
                index_size: 0,
                index_offset: 0,
            },
            index_data: vec![],
            index_offset: 0,
            mount_point: String::new(),
            entries: vec![],
            entry_paths: vec![],
        }
    }

    fn encrypted(&mut self) -> Result<bool, PakError> {
        self.load_pak_info()?;
        Ok(self.info.is_encrypted())
    }

    fn version(&mut self) -> Result<u32, PakError> {
        self.load_pak_info()?;
        Ok(self.info.version)
    }

    fn entries_count(&mut self) -> Result<u64, PakError> {
        self.load_entries()?;
        Ok(self.entries.len() as u64)
    }

    fn extract_entry_to_file(&mut self, entry_id: u64, output: &mut File) -> Result<(), PakError> {
        self.load_entries()?;
        let entries = &self.entries;
        let entry = entries[entry_id as usize].clone();

        if entry.num_of_blocks > 0 {
            for block in &entry.blocks {
                let mut compressed_data = vec![0u8; block.size() as usize];

                let bytes_read = read_file_at(&self.file, &mut compressed_data, block.offset())?;
                if bytes_read != block.size() as usize {
                    return Err(PakError::invalid_data(format!(
                        "Failed to read compressed chunk at {:08X}, read/expected: {}/{}",
                        block.offset(),
                        bytes_read,
                        block.size()
                    )));
                }

                if entry.encrypted != 0 {
                    xor_each_byte(&mut compressed_data, Self::DECRYPT_KEY);
                }

                if entry.compression_method != 1 {
                    return Err(PakError::invalid_data(format!(
                        "Unknown compression method '{}', only '1' is supported.",
                        entry.compression_method
                    )));
                }

                let decompressed_data =
                    zlib_decompress(&compressed_data, entry.compressed_block_size as usize)
                        .ok_or_else(|| std::io::Error::other("ZLIB decompression failed"))?;

                output.write_all(&decompressed_data)?;
            }
        } else {
            let mut file_offset = entry.file_offset + 74;
            let mut file_size = entry.file_size;

            while file_size > 0 {
                let bytes_to_read = std::cmp::min(file_size as usize, Self::CHUNK_SIZE);
                let mut decompressed_data = vec![0u8; bytes_to_read];
                let _bytes_read = read_file_at(&self.file, &mut decompressed_data, file_offset)?;

                if entry.encrypted != 0 {
                    xor_each_byte(&mut decompressed_data, Self::DECRYPT_KEY);
                }

                output.write_all(&decompressed_data)?;

                file_size -= bytes_to_read as u64;
                file_offset += bytes_to_read as u64;
            }
        }
        Ok(())
    }

    fn get_entry_path(&mut self, entry_id: u64) -> Result<String, PakError> {
        self.load_entry_paths()?;
        Ok(self.entry_paths[entry_id as usize].clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pak_reader::implements::open_paks_by_glob;
    use tempfile::TempDir;

    const GFP_PAKS_PATTERN: &str = "./test/normal/*.pak";
    const PAK_1: &str = "test/normal/game_patch_1.32.11.13846.pak";
    const PAK_2: &str = "test/normal/game_patch_1.32.11.13992.pak";

    #[test]
    fn test_get_pak_info() -> Result<(), Box<dyn std::error::Error>> {
        for (pak_path, mut pak) in open_paks_by_glob(GFP_PAKS_PATTERN, 10).unwrap() {
            println!("[{}]", pak_path.to_string_lossy());
            println!("IsEncrypted: {}", pak.encrypted()?);
            println!("Version: {}", pak.version()?);
            println!();
        }
        Ok(())
    }

    #[test]
    fn test_list_pak_entries() -> Result<(), Box<dyn std::error::Error>> {
        for (pak_path, mut pak) in open_paks_by_glob(GFP_PAKS_PATTERN, 10).unwrap() {
            println!(
                "Found {} entries in pack {}",
                pak.entries_count()?,
                pak_path.to_string_lossy()
            );

            for entry_id in 0..pak.entries_count()? {
                let path = pak.get_entry_path(entry_id)?;
                println!("[{}] {}", entry_id, path);
            }
        }
        Ok(())
    }

    #[test]
    fn test_extract_entry() -> Result<(), Box<dyn std::error::Error>> {
        let mut pak = GfpPakReaderV10::open(PAK_1)?;
        println!("Pak: {}", PAK_1);

        let temp_dir = TempDir::new()?;
        for entry_id in 0..pak.entries_count()? {
            let entry_path = pak.get_entry_path(entry_id)?;
            let output_path = temp_dir.path().join(entry_path);
            println!("Extracting to {}", output_path.to_string_lossy());

            if let Some(parent) = output_path.parent() {
                std::fs::create_dir_all(parent)?;
            }

            let mut output_file = File::create(&output_path)?;

            pak.extract_entry_to_file(entry_id, &mut output_file)?;
        }
        Ok(())
    }
}
