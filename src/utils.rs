use flate2::read::ZlibDecoder;
use std::fs::File;
use std::io;
use std::io::Read;

pub mod cli;
pub mod glob_ext;

pub fn xor_each_byte(data: &mut [u8], key: u8) {
    for byte in data.iter_mut() {
        *byte ^= key;
    }
}

pub fn read_file_at(file: &File, buf: &mut [u8], offset: u64) -> io::Result<usize> {
    #[cfg(target_os = "linux")]
    {
        use std::os::unix::fs::FileExt;
        return file.read_exact_at(buf, offset).map(|_| buf.len());
    }
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::fs::FileExt;
        file.seek_read(buf, offset)
    }
}

pub fn zlib_decompress(in_data: &[u8], out_size: usize) -> Option<Vec<u8>> {
    let mut decoder = ZlibDecoder::new(in_data);
    let mut output = Vec::with_capacity(out_size);

    decoder
        .read_to_end(&mut output)
        .map_or(None, |_| Some(output))
}

/// ```rust
/// use gfp::utils::utf16le_to_utf8_arr_inplace;
///
/// let mut buff = [0x41, 0x00, 0x2D, 0x4E]; // 'A' 和 '中'
/// let result = utf16le_to_utf8_arr_inplace(&mut buff);
/// println!("Result: {:?}", result);
/// println!("{:?}", buff);
/// assert_eq!(result, Ok(4));
/// assert_eq!(buff[0..4], [0x41, 0xE4, 0xB8, 0xAD]);
/// ```
pub fn utf16le_to_utf8_arr_inplace(buff: &mut [u8]) -> Result<usize, &'static str> {
    let mut i = 0;
    let mut j = 0;
    let len = buff.len();

    while i < len {
        if i + 1 >= len {
            return Err("Incomplete UTF-16 sequence");
        }

        // 读取UTF-16LE字符
        let unicode_char: u32 = (buff[i] as u32) | ((buff[i + 1] as u32) << 8);
        i += 2;

        // 将UTF-16LE转换为UTF-8
        if unicode_char <= 0x7F {
            buff[j] = unicode_char as u8;
            j += 1;
        } else if unicode_char <= 0x7FF {
            if j + 1 >= len {
                return Err("Output buffer too small");
            }
            buff[j] = 0xC0 | ((unicode_char >> 6) as u8);
            buff[j + 1] = 0x80 | ((unicode_char & 0x3F) as u8);
            j += 2;
        } else {
            if j + 2 >= len {
                return Err("Output buffer too small");
            }
            buff[j] = 0xE0 | ((unicode_char >> 12) as u8);
            buff[j + 1] = 0x80 | (((unicode_char >> 6) & 0x3F) as u8);
            buff[j + 2] = 0x80 | ((unicode_char & 0x3F) as u8);
            j += 3;
        }
    }

    if i < len {
        return Err("Input buffer not fully consumed");
    }

    Ok(j)
}

pub fn utf16le_to_utf8_inplace(utf16le: &mut Vec<u8>) {
    match utf16le_to_utf8_arr_inplace(utf16le) {
        Ok(len) => utf16le.truncate(len),
        Err(e) => panic!("{}", e),
    }
}

pub mod file_reader {
    pub struct VecCursor<'a, T> {
        pub buffer: &'a Vec<T>,
        pub offset: usize,
    }

    impl<T: Clone> VecCursor<'_, T> {
        pub fn new(data: &'_ Vec<T>) -> VecCursor<'_, T> {
            VecCursor::<'_, T> {
                buffer: data,
                offset: 0,
            }
        }
        pub fn new_with_offset(data: &'_ Vec<T>, offset: usize) -> VecCursor<'_, T> {
            VecCursor::<'_, T> {
                buffer: data,
                offset,
            }
        }

        pub fn read_nocheck<const N: usize>(&mut self) -> &[T; N] {
            let slice = &self.buffer[self.offset..(self.offset + N)];
            self.move_by(N);
            slice.try_into().unwrap()
        }

        pub fn read<const N: usize>(&mut self) -> Result<&[T; N], std::io::Error> {
            if self.offset + N > self.buffer.len() {
                Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "Read past end of buffer",
                ))
            } else {
                Ok(self.read_nocheck::<N>())
            }
        }

        pub fn read_nocheck_dyn(&mut self, length: usize) -> Vec<T> {
            let slice = &self.buffer[self.offset..(self.offset + length)];
            self.move_by(length);
            slice.to_vec()
        }

        pub fn read_dyn(&mut self, length: usize) -> Result<Vec<T>, std::io::Error> {
            if self.offset + length > self.buffer.len() {
                Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "Read past end of buffer",
                ))
            } else {
                Ok(self.read_nocheck_dyn(length))
            }
        }

        pub fn move_to(&mut self, offset: usize) {
            self.offset = offset;
        }

        pub fn move_by(&mut self, offset: usize) {
            self.offset += offset;
        }
    }
}
