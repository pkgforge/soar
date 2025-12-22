//! Error types for soar-utils.

use std::path::PathBuf;

use miette::Diagnostic;
use thiserror::Error;

/// Error type for byte parsing operations.
#[derive(Error, Diagnostic, Debug)]
pub enum BytesError {
    #[error("Failed to parse '{input}' as bytes: {reason}")]
    #[diagnostic(
        code(soar_utils::bytes::parse),
        help("Use a valid byte format like '1KB', '2MB', or '3GB'")
    )]
    ParseFailed { input: String, reason: String },
}

/// Error type for hash operations.
#[derive(Error, Diagnostic, Debug)]
pub enum HashError {
    #[error("Failed to read file '{path}'")]
    #[diagnostic(
        code(soar_utils::hash::read),
        help("Check if the file exists and you have read permissions")
    )]
    ReadFailed {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
}

/// Error type for path operations.
#[derive(Error, Diagnostic, Debug)]
pub enum PathError {
    #[error("Failed to get current directory")]
    #[diagnostic(
        code(soar_utils::path::cwd),
        help("Check if the current directory still exists")
    )]
    FailedToGetCurrentDir {
        #[source]
        source: std::io::Error,
    },

    #[error("Path is empty")]
    #[diagnostic(
        code(soar_utils::path::empty),
        help("Provide a non-empty path")
    )]
    Empty,

    #[error("Environment variable '{var}' not set in '{input}'")]
    #[diagnostic(
        code(soar_utils::path::env_var),
        help("Set the environment variable or use a different path")
    )]
    MissingEnvVar { var: String, input: String },

    #[error("Unclosed variable expression starting at '{input}'")]
    #[diagnostic(
        code(soar_utils::path::unclosed_var),
        help("Close the variable expression with '}}'")
    )]
    UnclosedVariable { input: String },
}

/// Error type for filesystem operations.
#[derive(Error, Diagnostic, Debug)]
pub enum FileSystemError {
    #[error("Failed to read file '{path}'")]
    #[diagnostic(
        code(soar_utils::fs::read_file),
        help("Check if the file exists and you have read permissions")
    )]
    ReadFile {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("Failed to write file '{path}'")]
    #[diagnostic(
        code(soar_utils::fs::write_file),
        help("Check if you have write permissions to the directory")
    )]
    WriteFile {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("Failed to create file '{path}'")]
    #[diagnostic(
        code(soar_utils::fs::create_file),
        help("Check if the directory exists and you have write permissions")
    )]
    CreateFile {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("Failed to remove file '{path}'")]
    #[diagnostic(
        code(soar_utils::fs::remove_file),
        help("Check if you have write permissions to the file")
    )]
    RemoveFile {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("Failed to read directory '{path}'")]
    #[diagnostic(
        code(soar_utils::fs::read_dir),
        help("Check if the directory exists and you have read permissions")
    )]
    ReadDirectory {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("Failed to create directory '{path}'")]
    #[diagnostic(
        code(soar_utils::fs::create_dir),
        help("Check if the parent directory exists and you have write permissions")
    )]
    CreateDirectory {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("Failed to remove directory '{path}'")]
    #[diagnostic(
        code(soar_utils::fs::remove_dir),
        help("Check if the directory is empty and you have write permissions")
    )]
    RemoveDirectory {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("Failed to create symlink from '{from}' to '{target}'")]
    #[diagnostic(
        code(soar_utils::fs::create_symlink),
        help("Check if you have write permissions and the target doesn't already exist")
    )]
    CreateSymlink {
        from: PathBuf,
        target: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("Failed to remove symlink '{path}'")]
    #[diagnostic(
        code(soar_utils::fs::remove_symlink),
        help("Check if you have write permissions")
    )]
    RemoveSymlink {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("Failed to read symlink '{path}'")]
    #[diagnostic(
        code(soar_utils::fs::read_symlink),
        help("Check if the symlink exists")
    )]
    ReadSymlink {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("Path '{path}' not found")]
    #[diagnostic(
        code(soar_utils::fs::not_found),
        help("Check if the path exists")
    )]
    NotFound { path: PathBuf },

    #[error("'{path}' is not a directory")]
    #[diagnostic(
        code(soar_utils::fs::not_a_dir),
        help("Provide a path to a directory")
    )]
    NotADirectory { path: PathBuf },

    #[error("'{path}' is not a file")]
    #[diagnostic(
        code(soar_utils::fs::not_a_file),
        help("Provide a path to a file")
    )]
    NotAFile { path: PathBuf },
}

/// Context for filesystem operations.
pub struct IoContext {
    path: PathBuf,
    operation: IoOperation,
}

/// Type of filesystem operation.
#[derive(Debug, Clone)]
pub enum IoOperation {
    ReadFile,
    WriteFile,
    CreateFile,
    RemoveFile,
    CreateDirectory,
    RemoveDirectory,
    ReadDirectory,
    CreateSymlink { target: PathBuf },
    RemoveSymlink,
    ReadSymlink,
}

impl IoContext {
    pub fn new(path: PathBuf, operation: IoOperation) -> Self {
        Self { path, operation }
    }

    pub fn read_file<P: Into<PathBuf>>(path: P) -> Self {
        Self::new(path.into(), IoOperation::ReadFile)
    }

    pub fn write_file<P: Into<PathBuf>>(path: P) -> Self {
        Self::new(path.into(), IoOperation::WriteFile)
    }

    pub fn create_file<P: Into<PathBuf>>(path: P) -> Self {
        Self::new(path.into(), IoOperation::CreateFile)
    }

    pub fn remove_file<P: Into<PathBuf>>(path: P) -> Self {
        Self::new(path.into(), IoOperation::RemoveFile)
    }

    pub fn read_directory<P: Into<PathBuf>>(path: P) -> Self {
        Self::new(path.into(), IoOperation::ReadDirectory)
    }

    pub fn create_directory<P: Into<PathBuf>>(path: P) -> Self {
        Self::new(path.into(), IoOperation::CreateDirectory)
    }

    pub fn remove_directory<P: Into<PathBuf>>(path: P) -> Self {
        Self::new(path.into(), IoOperation::RemoveDirectory)
    }

    pub fn read_symlink<P: Into<PathBuf>>(path: P) -> Self {
        Self::new(path.into(), IoOperation::ReadSymlink)
    }

    pub fn create_symlink<P: Into<PathBuf>, T: Into<PathBuf>>(from: P, target: T) -> Self {
        Self::new(
            from.into(),
            IoOperation::CreateSymlink {
                target: target.into(),
            },
        )
    }

    pub fn remove_symlink<P: Into<PathBuf>>(path: P) -> Self {
        Self::new(path.into(), IoOperation::RemoveSymlink)
    }

    pub fn operation(&self) -> &IoOperation {
        &self.operation
    }
}

impl From<(IoContext, std::io::Error)> for FileSystemError {
    fn from((ctx, source): (IoContext, std::io::Error)) -> Self {
        match ctx.operation {
            IoOperation::ReadFile => FileSystemError::ReadFile {
                path: ctx.path,
                source,
            },
            IoOperation::WriteFile => FileSystemError::WriteFile {
                path: ctx.path,
                source,
            },
            IoOperation::CreateFile => FileSystemError::CreateFile {
                path: ctx.path,
                source,
            },
            IoOperation::RemoveFile => FileSystemError::RemoveFile {
                path: ctx.path,
                source,
            },
            IoOperation::CreateDirectory => FileSystemError::CreateDirectory {
                path: ctx.path,
                source,
            },
            IoOperation::RemoveDirectory => FileSystemError::RemoveDirectory {
                path: ctx.path,
                source,
            },
            IoOperation::ReadDirectory => FileSystemError::ReadDirectory {
                path: ctx.path,
                source,
            },
            IoOperation::CreateSymlink { target } => FileSystemError::CreateSymlink {
                from: ctx.path,
                target,
                source,
            },
            IoOperation::RemoveSymlink => FileSystemError::RemoveSymlink {
                path: ctx.path,
                source,
            },
            IoOperation::ReadSymlink => FileSystemError::ReadSymlink {
                path: ctx.path,
                source,
            },
        }
    }
}

/// Extension trait for adding path context to IO results.
pub trait IoResultExt<T> {
    fn with_path<P: Into<PathBuf>>(self, path: P, operation: IoOperation) -> FileSystemResult<T>;
}

impl<T> IoResultExt<T> for std::io::Result<T> {
    fn with_path<P: Into<PathBuf>>(self, path: P, operation: IoOperation) -> FileSystemResult<T> {
        self.map_err(|e| {
            let ctx = IoContext::new(path.into(), operation);
            (ctx, e).into()
        })
    }
}

/// Combined error type for all utils errors.
#[derive(Error, Diagnostic, Debug)]
pub enum UtilsError {
    #[error(transparent)]
    #[diagnostic(transparent)]
    Bytes(#[from] BytesError),

    #[error(transparent)]
    #[diagnostic(transparent)]
    Path(#[from] PathError),

    #[error(transparent)]
    #[diagnostic(transparent)]
    FileSystem(#[from] FileSystemError),
}

pub type BytesResult<T> = std::result::Result<T, BytesError>;
pub type FileSystemResult<T> = std::result::Result<T, FileSystemError>;
pub type HashResult<T> = std::result::Result<T, HashError>;
pub type PathResult<T> = std::result::Result<T, PathError>;
pub type UtilsResult<T> = std::result::Result<T, UtilsError>;

#[cfg(test)]
mod tests {
    use std::io;

    use super::*;

    #[test]
    fn test_bytes_error_display() {
        let error = BytesError::ParseFailed {
            input: "test".to_string(),
            reason: "invalid".to_string(),
        };
        assert_eq!(
            error.to_string(),
            "Failed to parse 'test' as bytes: invalid"
        );
    }

    #[test]
    fn test_hash_error_display_and_source() {
        let io_error = io::Error::new(io::ErrorKind::NotFound, "file not found");
        let error = HashError::ReadFailed {
            path: PathBuf::from("/test"),
            source: io_error,
        };
        assert_eq!(error.to_string(), "Failed to read file '/test'");
    }

    #[test]
    fn test_path_error_display() {
        let empty_error = PathError::Empty;
        assert_eq!(empty_error.to_string(), "Path is empty");

        let missing_env_var_error = PathError::MissingEnvVar {
            var: "VAR".to_string(),
            input: "$VAR".to_string(),
        };
        assert_eq!(
            missing_env_var_error.to_string(),
            "Environment variable 'VAR' not set in '$VAR'"
        );

        let unclosed_variable_error = PathError::UnclosedVariable {
            input: "${VAR".to_string(),
        };
        assert_eq!(
            unclosed_variable_error.to_string(),
            "Unclosed variable expression starting at '${VAR'"
        );
    }

    #[test]
    fn test_file_system_error_display() {
        let io_error = io::Error::new(io::ErrorKind::PermissionDenied, "permission denied");
        let file_error = FileSystemError::ReadFile {
            path: PathBuf::from("/file"),
            source: io_error,
        };
        assert_eq!(file_error.to_string(), "Failed to read file '/file'");

        let not_a_dir_error = FileSystemError::NotADirectory {
            path: PathBuf::from("/path"),
        };
        assert_eq!(not_a_dir_error.to_string(), "'/path' is not a directory");
    }
}
