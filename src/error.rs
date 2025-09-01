use std::ffi::{FromVecWithNulError, IntoStringError};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum PakError {
    #[error("Data not loaded yet")]
    #[deprecated]
    DataNotLoadedYet,

    #[error("Invalid data: {}", .0)]
    InvalidData(String),

    #[error("IO error: {:?}", .0)]
    Io(std::io::Error),

    #[error("Other: {}", .0)]
    Other(String),
}

impl From<std::io::Error> for PakError {
    fn from(error: std::io::Error) -> Self {
        PakError::Io(error)
    }
}
impl From<FromVecWithNulError> for PakError {
    fn from(error: FromVecWithNulError) -> Self {
        PakError::InvalidData(error.to_string())
    }
}
impl From<IntoStringError> for PakError {
    fn from(error: IntoStringError) -> Self {
        PakError::InvalidData(error.to_string())
    }
}

impl PakError {
    pub fn invalid_data(message: impl AsRef<str>) -> PakError {
        PakError::InvalidData(message.as_ref().to_string())
    }
}
