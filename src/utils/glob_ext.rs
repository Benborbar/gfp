use glob::{GlobResult, MatchOptions, Paths, glob, glob_with};

/// A wrapper around glob `Paths` iterator that applies a mapping function
/// to each result, transforming them into a different type.
///
/// This struct should be created by [`glob_mapper`] and [`glob_mapper_with`].
pub struct GlobMapper<F> {
    paths: Paths,
    mapper: F,
}

impl<T, F> Iterator for GlobMapper<F>
where
    F: Fn(GlobResult) -> Option<T>,
{
    type Item = T;

    fn next(&mut self) -> Option<Self::Item> {
        while let Some(next) = self.paths.next() {
            if let Some(item) = (self.mapper)(next) {
                return Some(item);
            }
        }
        None
    }
}

/// Create a function that return a [`GlobMapper`] with default options.
///
/// Takes a mapping function and returns a closure that can be used to create a [`GlobMapper`] with default options.
///
/// ## Arguments
///
/// - `mapper` - A function that takes a [`GlobResult`] and returns an `Option<T>`
///   - **Argument** `result` - A [`GlobResult`]
///   - **Return** An `Option<T>` that contains the mapped value. If `None`, the iterator will skip this value.
///
/// ## Return
///
/// A function that takes a glob pattern string and returns a [`GlobMapper`] or
/// a [`glob::PatternError`].
///
/// ## Example
///
/// ```rust
/// use std::path::PathBuf;
/// use glob::GlobResult;
/// use gfp::utils::glob_ext::glob_mapper;
///
/// let mapper_fn = glob_mapper(|result| {
///     // return: Option<PathBuf>
///     result.ok()             // return a mapped value wrap in `Option`
/// });
///
/// for item in mapper_fn("**/*.rs").unwrap() {
///     // item: PathBuf
///     println!("{:?}", item); // take the mapped value
/// }
/// ```
pub fn glob_mapper<'a, T, F>(
    mapper: F,
) -> impl Fn(&str) -> Result<GlobMapper<F>, glob::PatternError>
where
    F: Fn(GlobResult) -> Option<T> + 'a + Copy,
{
    move |pattern| {
        Ok(GlobMapper {
            paths: glob(pattern)?,
            mapper,
        })
    }
}

/// Create a function that returns a [`GlobMapper`] with custom options.
///
/// Take a mapping function and return a closure that can be used to create a [`GlobMapper`] with custom [`MatchOptions`].
///
/// # Arguments
///
/// * `mapper` - A function that takes a [`GlobResult`] and returns an `Option<T>`
///
/// # Return
///
/// A function that takes a glob pattern string and [`MatchOptions`], and returns a [`GlobMapper`] or a [`glob::PatternError`].
///
/// # Example
///
/// ```rust
/// use glob::MatchOptions;
/// use gfp::utils::glob_ext::glob_mapper_with;
///
/// let mapper_fn = glob_mapper_with(|result| {
///     // return: Option<PathBuf>
///     result.ok()             // return a mapped value wrap in `Option`
/// });
///
/// for item in  mapper_fn("**/*.rs", MatchOptions::new()).unwrap() {
///     // item: PathBuf
///     println!("{:?}", item); // take the mapped value
/// }
/// ```
pub fn glob_mapper_with<'a, T, F>(
    mapper: F,
) -> impl Fn(&str, MatchOptions) -> Result<GlobMapper<F>, glob::PatternError>
where
    F: Fn(GlobResult) -> Option<T> + 'a + Copy,
{
    move |pattern, options| {
        Ok(GlobMapper {
            paths: glob_with(pattern, options)?,
            mapper,
        })
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use std::fs;
    use std::fs::File;
    use tempfile::TempDir;

    #[test]
    fn test_glob_mapper_basic() -> Result<(), Box<dyn std::error::Error>> {
        let temp_dir = TempDir::new()?;
        let temp_path = temp_dir.path();

        let test_file1 = temp_path.join("test1.pak");
        let test_file2 = temp_path.join("test2.pak");
        let test_file3 = temp_path.join("test3.txt");

        fs::write(&test_file1, b"test pak content 1")?;
        fs::write(&test_file2, b"test pak content 2")?;
        fs::write(&test_file3, b"not a pak file")?;

        let pattern_str = temp_path.join("*.pak").to_string_lossy().to_string();

        let my_iter = glob_mapper(|result: GlobResult| match result {
            Ok(entry) => {
                if !entry.extension().map_or(false, |ext| ext == "pak") {
                    None
                } else {
                    File::open(&entry).ok()
                }
            }
            Err(e) => {
                eprintln!("Error accessing entry: {:?}", e);
                None
            }
        });

        let mut pak_count = 0;
        for _pak in my_iter(&pattern_str)? {
            pak_count += 1;
        }

        assert_eq!(pak_count, 2);

        Ok(())
    }

    #[test]
    fn test_glob_mapper_empty() -> Result<(), Box<dyn std::error::Error>> {
        let temp_dir = TempDir::new()?;
        let temp_path = temp_dir.path();

        let test_file1 = temp_path.join("test1.txt");
        let test_file2 = temp_path.join("test2.log");

        fs::write(&test_file1, b"test text content")?;
        fs::write(&test_file2, b"log content")?;

        let pattern_str = temp_path.join("*.pak").to_string_lossy().to_string();

        let my_iter = glob_mapper(|result: GlobResult| match result {
            Ok(entry) => File::open(&entry).ok(),
            Err(e) => {
                eprintln!("Error accessing entry: {:?}", e);
                None
            }
        });

        let mut pak_count = 0;
        for _pak in my_iter(&pattern_str)? {
            pak_count += 1;
        }

        assert_eq!(pak_count, 0);

        Ok(())
    }

    #[test]
    fn test_glob_mapper_with_options() -> Result<(), Box<dyn std::error::Error>> {
        let temp_dir = TempDir::new()?;
        let temp_path = temp_dir.path();

        let test_file1 = temp_path.join("test_pak.pak");
        let test_file2 = temp_path.join("test_data.pak");
        let test_file3 = temp_path.join("test_files.pak");

        fs::write(&test_file1, b"test pak content 1")?;
        fs::write(&test_file2, b"test pak content 2")?;
        fs::write(&test_file3, b"test pak content 3")?;

        let pattern_str = temp_path.join("test_*.pak").to_string_lossy().to_string();

        let options = MatchOptions {
            case_sensitive: false,
            require_literal_separator: false,
            require_literal_leading_dot: false,
        };

        let my_iter = glob_mapper_with(|result: GlobResult| match result {
            Ok(entry) => File::open(&entry).ok(),
            Err(e) => {
                eprintln!("Error accessing entry: {:?}", e);
                None
            }
        });

        let mut pak_count = 0;
        for _pak in my_iter(&pattern_str, options)? {
            pak_count += 1;
        }

        assert_eq!(pak_count, 3);

        Ok(())
    }

    #[test]
    fn test_glob_mapper_error_handling() -> Result<(), Box<dyn std::error::Error>> {
        let pattern_str = "nonexistent_directory_0ds9fas0930i0kbdofgids/*.pak";

        let my_iter = glob_mapper(|result: GlobResult| match result {
            Ok(entry) => File::open(&entry).ok(),
            Err(_e) => None,
        });

        let mut pak_count = 0;
        for _pak in my_iter(pattern_str)? {
            pak_count += 1;
        }

        assert_eq!(pak_count, 0);

        Ok(())
    }
}
