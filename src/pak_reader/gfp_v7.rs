use crate::error::PakError;
use crate::pak_reader::PakReader;
use crate::utils::file_reader::VecCursor;
use crate::utils::{read_file_at, utf16le_to_utf8_inplace, xor_each_byte, zlib_decompress};
use std::ffi::CString;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom, Write};

/// Pak file header information for avatar pak files
/// Total size: 45 bytes
#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
struct RawPakInfo {
    encrypted: u8,   // 1 byte
    magic: u32,      // 4 bytes
    version: u32,    // 4 bytes
    hash: [u8; 20],  // 20 bytes
    index_size: u64, // 8 bytes
    offset: u64,     // 8 bytes
}

impl RawPakInfo {
    /// Check if pak file is encrypted
    fn is_encrypted(&self) -> bool {
        self.encrypted != 0
    }
}

/// Compression block information
#[derive(Debug, Clone, Copy)]
struct CompressionBlock {
    start: u64,
    end: u64,
}

impl CompressionBlock {
    /// Get block offset in file
    fn offset(&self) -> u64 {
        self.start
    }

    /// Get block size
    fn size(&self) -> u64 {
        self.end - self.start
    }
}

/// File entry in pak
#[derive(Debug, Clone)]
struct Entry {
    file_hash: [u8; 20],
    file_offset: u64,
    file_size: u64,
    compression_method: u32,
    compressed_length: u64,
    dummy: [u8; 21],
    num_of_blocks: u32,
    blocks: Vec<CompressionBlock>,
    compressed_block_size: u32,
    encrypted: u8,
    path: String,
}

/// 参考 `src/c/gfp_avatar.c`
pub struct GfpPakReaderV7 {
    pub file: File,

    is_info_loaded: bool,
    is_entries_loaded: bool,

    // Stage: info
    info: RawPakInfo,

    // Stage: entries
    index_data: Vec<u8>,
    index_offset: usize,
    mount_point: String,
    entries: Vec<Entry>,
}

impl GfpPakReaderV7 {
    const PAK_INFO_SIZE: usize = std::mem::size_of::<RawPakInfo>();
    const OFFSET_XOR_KEY: u64 = 0xD74AF37FAA6B020D;
    const SIZE_XOR_KEY: u64 = 0x8924B0E3298B7069;
    const ENCRYPTED_XOR_KEY: u8 = 0x6C;
    const DECRYPT_KEY: u8 = 0x79;
    const CHUNK_SIZE: usize = 65536;
    const HASH_KEY: [u8; 20] = [
        0x9B, 0x31, 0x24, 0x61, 0xCB, 0xD3, 0xF5, 0x18, 0x20, 0xA1, 0x1B, 0xFB, 0xFD, 0x40, 0xB6,
        0x00, 0x1E, 0x53, 0x5C, 0x24,
    ];

    /// Load pak file header information
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

        // Deobfuscation
        self.info.encrypted ^= Self::ENCRYPTED_XOR_KEY;
        for i in 0..20 {
            self.info.hash[i] ^= Self::HASH_KEY[i];
        }
        self.info.offset ^= Self::OFFSET_XOR_KEY;
        self.info.index_size ^= Self::SIZE_XOR_KEY;
        self.is_info_loaded = true;
        Ok(())
    }

    /// Load file entries from pak
    fn load_entries(&mut self) -> Result<(), PakError> {
        self.load_pak_info()?;

        if self.is_entries_loaded {
            return Ok(());
        }

        // Index data
        {
            let mut index_data: Vec<u8> = vec![0u8; self.info.index_size as usize];
            read_file_at(&mut self.file, &mut index_data, self.info.offset)?;

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
                    path: String::new(),
                };
                entry_count as usize
            ];

            for entry_id in 0..entry_count as usize {
                let entry = &mut self.entries[entry_id];

                let entry_path_size = i32::from_le_bytes(*index_cursor.read::<4>()?);

                match entry_path_size {
                    8192.. => {
                        return Err(PakError::invalid_data(format!(
                            "Entry path too long: {}",
                            entry_path_size
                        )));
                    }
                    ..0 => {
                        let mut data = index_cursor.read_dyn((-entry_path_size * 2) as usize)?;
                        utf16le_to_utf8_inplace(&mut data);
                        entry.path = CString::from_vec_with_nul(data)?.into_string()?;
                    }
                    _ => {
                        let data = index_cursor.read_dyn(entry_path_size as usize)?;
                        entry.path = CString::from_vec_with_nul(data)?.into_string()?;
                    }
                }

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
        }
        self.is_entries_loaded = true;

        Ok(())
    }
}

impl PakReader for GfpPakReaderV7 {
    /// Create a new GfpAvatarPakReader instance
    fn new(file: File) -> Self {
        Self {
            file,
            is_info_loaded: false,
            is_entries_loaded: false,
            info: RawPakInfo {
                encrypted: 0,
                magic: 0,
                version: 0,
                hash: [0; 20],
                index_size: 0,
                offset: 0,
            },
            index_data: vec![],
            index_offset: 0,
            mount_point: String::new(),
            entries: vec![],
        }
    }

 

    /// Check if pak file is encrypted
    fn encrypted(&mut self) -> Result<bool, PakError> {
        self.load_pak_info()?;
        Ok(self.info.is_encrypted())
    }

    /// Get pak file version
    fn version(&mut self) -> Result<u32, PakError> {
        self.load_pak_info()?;
        Ok(self.info.version)
    }

    /// Get number of entries in pak file
    fn entries_count(&mut self) -> Result<u64, PakError> {
        self.load_entries()?;
        Ok(self.entries.len() as u64)
    }

    /// Extract an entry to a file
    fn extract_entry_to_file(&mut self, entry_id: u64, output: &mut File) -> Result<(), PakError> {
        self.load_entries()?;
        let entry = self.entries[entry_id as usize].clone();

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

    /// Get entry path by ID
    fn get_entry_path(&mut self, entry_id: u64) -> Result<String, PakError> {
        self.load_entries()?;
        Ok(self.entries[entry_id as usize].path.clone())
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::pak_reader::implements::open_paks_by_glob;
    use tempfile::TempDir;
    
    const GFP_AVATAR_PAKS_PATTERN: &str = "./test/avatar/*.pak";
    const AVATAR_PAK_1: &str = "test/avatar/onreadypak_405399.pak";
    const AVATAR_PAK_2: &str = "test/avatar/onreadypak_101005004.pak";

    #[test]
    fn test_get_pak_info() -> Result<(), Box<dyn std::error::Error>> {
        for (pak_path, mut pak) in open_paks_by_glob(GFP_AVATAR_PAKS_PATTERN, 7).unwrap() {
            println!("[{}]", pak_path.to_string_lossy());
            println!("IsEncrypted: {}", pak.encrypted()?);
            println!("Version: {}", pak.version()?);
            println!();
        }
        Ok(())
    }
    #[test]
    fn test_load_entries() -> Result<(), Box<dyn std::error::Error>> {
        for (pak_path, mut pak) in open_paks_by_glob(GFP_AVATAR_PAKS_PATTERN, 7).unwrap() {
            println!(
                "{} entries in {}",
                pak.entries_count()?,
                pak_path.to_str().unwrap()
            );
        }
        Ok(())
    }

    #[test]
    fn test_list_pak_entries() -> Result<(), Box<dyn std::error::Error>> {
        for (pak_path, mut pak) in open_paks_by_glob(GFP_AVATAR_PAKS_PATTERN, 7).unwrap() {
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
        let mut pak = GfpPakReaderV7::open(AVATAR_PAK_1)?;
        println!("Pak: {}", AVATAR_PAK_1);

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
