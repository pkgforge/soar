use std::path::Path;

use crate::error::{HashError, HashResult};

/// Calculates the checksum of a file.
///
/// This method reads the contents of a file and computes a checksum, which is returned as a
/// hex-encoded string. The specific hashing algorithm depends on the implementation. The
/// default implementation uses the `blake3` crate.
///
/// # Arguments
///
/// * `file_path` - The path to the file to calculate the checksum for.
///
/// # Errors
///
/// * [`HashError::ReadFailed`] if the file cannot be read.
///
/// # Example
///
/// ```no_run
/// use soar_utils::error::HashResult;
/// use soar_utils::hash::calculate_checksum;
///
/// fn main() -> HashResult<()> {
///     let checksum = calculate_checksum("/path/to/file")?;
///     println!("Checksum is {}", checksum);
///     Ok(())
/// }
/// ```
pub fn calculate_checksum<P: AsRef<Path>>(file_path: P) -> HashResult<String> {
    let file_path = file_path.as_ref();
    let mut hasher = blake3::Hasher::new();
    hasher.update_mmap(file_path).map_err(|err| {
        HashError::ReadFailed {
            path: file_path.to_path_buf(),
            source: err,
        }
    })?;
    Ok(hasher.finalize().to_hex().to_string())
}

/// Verifies the checksum of a file against an expected value.
///
/// This method calculates the checksum of the given file and compares it case-insensitively
/// against the `expected` checksum string.
///
/// # Arguments
///
/// * `file_path` - The path to the file to verify the checksum for.
/// * `expected` - The expected checksum.
///
/// # Errors
///
/// * [`HashError::ReadFailed`] if the file cannot be read.
///
/// # Example
///
/// ```no_run
/// use soar_utils::error::HashResult;
/// use soar_utils::hash::verify_checksum;
///
/// fn main() -> HashResult<()> {
///     let result = verify_checksum("file.dat", "1234567890abcdef")?;
///     println!("Checksum matches: {}", result);
///     Ok(())
/// }
/// ```
pub fn verify_checksum<P: AsRef<Path>>(file_path: P, expected: &str) -> HashResult<bool> {
    let file_path = file_path.as_ref();
    let actual = calculate_checksum(file_path)?;
    Ok(actual.eq_ignore_ascii_case(expected))
}

#[cfg(test)]
mod tests {
    use std::io::Write;

    use tempfile::NamedTempFile;

    use super::{calculate_checksum, verify_checksum};

    #[test]
    fn test_calculate_checksum() {
        let mut file = NamedTempFile::new().unwrap();
        file.write_all(b"hello world\n").unwrap();
        let path = file.path();

        let checksum = calculate_checksum(path).unwrap();
        assert_eq!(
            checksum,
            "dc5a4edb8240b018124052c330270696f96771a63b45250a5c17d3000e823355"
        );
    }

    #[test]
    fn test_verify_checksum_valid() {
        let mut file = NamedTempFile::new().unwrap();
        file.write_all(b"hello world\n").unwrap();
        let path = file.path();

        let result = verify_checksum(
            path,
            "dc5a4edb8240b018124052c330270696f96771a63b45250a5c17d3000e823355",
        )
        .unwrap();
        assert!(result);
    }

    #[test]
    fn test_verify_checksum_invalid() {
        let mut file = NamedTempFile::new().unwrap();
        file.write_all(b"hello world").unwrap();
        let path = file.path();

        let result = verify_checksum(path, "invalid-checksum").unwrap();
        assert!(!result);
    }

    #[test]
    fn test_calculate_checksum_file_not_found() {
        let result = calculate_checksum("/path/to/nonexistent/file");
        assert!(result.is_err());
    }

    #[test]
    fn test_verify_checksum_file_not_found() {
        let result = verify_checksum("/path/to/nonexistent/file", "any-checksum");
        assert!(result.is_err());
    }

    #[test]
    fn test_calculate_checksum_on_directory() {
        let dir = tempfile::tempdir().unwrap();
        let result = calculate_checksum(dir.path());
        assert!(result.is_err());
    }
}
