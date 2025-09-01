pub mod gfp_v10;
pub mod gfp_v7;

use crate::error::PakError;
use std::fs::File;
use std::path::Path;

pub trait PakReader {
    // Stages
    fn new(file: File) -> Self
    where
        Self: Sized;
    fn open<P: AsRef<Path>>(path: P) -> Result<Box<dyn PakReader>, std::io::Error>
    where
        Self: Sized + 'static,
    {
        Ok(Box::new(Self::new(File::open(path)?)))
    }

    // pak info
    /// [`Self::load_pak_info`]
    fn encrypted(&mut self) -> Result<bool, PakError>;
    /// [`Self::load_pak_info`]
    fn version(&mut self) -> Result<u32, PakError>;

    /// [`Self::load_entries`]
    fn entries_count(&mut self) -> Result<u64, PakError>;

    /// [`Self::load_entries`]
    fn extract_entry_to_file(&mut self, entry_id: u64, output: &mut File) -> Result<(), PakError>;

    /// [`Self::load_entries`]
    fn extract_entry_to_path<P: AsRef<Path>>(
        &mut self,
        entry_id: u64,
        output: P,
    ) -> Result<(), PakError>
    where
        Self: Sized,
    {
        self.extract_entry_to_file(entry_id, &mut File::create(output)?)
    }
    /// [`Self::load_entry_paths`]
    fn get_entry_path(&mut self, entry_id: u64) -> Result<String, PakError>;
}

pub mod implements {
    use crate::error::PakError;
    use crate::pak_reader::gfp_v10::GfpPakReaderV10;
    use crate::pak_reader::gfp_v7::GfpPakReaderV7;
    use crate::pak_reader::PakReader;
    use crate::utils::glob_ext::glob_mapper;
    use glob::PatternError;
    use std::path::{Path, PathBuf};
    
    pub fn open_pak<P: AsRef<Path>>(path: P, varient: i32) -> Result<Box<dyn PakReader>, PakError> {
        Ok(match varient {
            7 => GfpPakReaderV7::open(path)?,
            10 => GfpPakReaderV10::open(path)?,
            _ => panic!("Invalid varient: {}", varient),
        })
    }

    pub fn open_paks_by_glob(
        pattern: &str,
        varient: i32,
    ) -> Result<impl Iterator<Item = (PathBuf, Box<dyn PakReader>)>, PatternError> {
        glob_mapper(move |result| match result {
            Ok(pak_path) => match open_pak(&pak_path, varient) {
                Ok(pak) => Some((pak_path, pak)),
                Err(e) => {
                    eprintln!("Error opening pak file: {:?}", e);
                    None
                }
            },
            Err(e) => {
                eprintln!("Error accessing entry: {:?}", e);
                None
            }
        })(pattern)
    }
}
