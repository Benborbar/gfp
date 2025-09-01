/// ```rust
/// use std::path::PathBuf;
/// use gfp::utils::cli::prepare_file_pattern;
///
/// assert_eq!(prepare_file_pattern("."), "./**/*.pak".to_string());
/// assert_eq!(prepare_file_pattern("./Paks"), "./Paks/**/*.pak".to_string());
/// assert_eq!(prepare_file_pattern("./Paks/"), "./Paks/**/*.pak".to_string());
/// assert_eq!(prepare_file_pattern("**/*.pak"), "**/*.pak".to_string());
/// assert_eq!(prepare_file_pattern("./Paks/**/*.pak"), "./Paks/**/*.pak".to_string());
/// assert_eq!(prepare_file_pattern("./Paks/abc.pak"), "./Paks/abc.pak".to_string());
/// ```
pub fn prepare_file_pattern(file_pattern: impl AsRef<str>) -> String {
    let mut file_pattern = file_pattern.as_ref().to_string();
    if file_pattern.ends_with(".pak") {
        file_pattern
    } else {
        if !file_pattern.ends_with(|c| c == '/' || c == '\\') {
            file_pattern += "/";
        }
        file_pattern + "**/*.pak"
    }
}
