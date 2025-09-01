#[cfg(not(target_pointer_width = "64"))]
compile_error!("This crate only supports 64-bit platforms");

pub mod error;
pub mod pak_reader;
pub mod utils;
