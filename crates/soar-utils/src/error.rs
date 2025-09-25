use std::{error::Error, fmt, path::PathBuf};

#[derive(Debug)]
pub enum BytesError {
    ParseFailed { input: String, reason: String },
}

impl fmt::Display for BytesError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            BytesError::ParseFailed { input, reason } => {
                write!(f, "Failed to parse `{input}` as bytes: {reason}")
            }
        }
    }
}

#[derive(Debug)]
pub enum HashError {
    ReadFailed {
        path: PathBuf,
        source: std::io::Error,
    },
}

impl fmt::Display for HashError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            HashError::ReadFailed { path, source } => {
                write!(f, "Failed to read file `{}`: {source}", path.display())
            }
        }
    }
}

impl Error for HashError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            HashError::ReadFailed { source, .. } => Some(source),
        }
    }
}

impl Error for BytesError {}

#[derive(Debug)]
pub enum PathError {
    FailedToGetCurrentDir { source: std::io::Error },

    Empty,

    MissingEnvVar { var: String, input: String },

    UnclosedVariable { input: String },
}

impl fmt::Display for PathError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PathError::Empty => write!(f, "Path is empty"),
            PathError::FailedToGetCurrentDir { source } => {
                write!(f, "Failed to get current directory: {source}")
            }
            PathError::UnclosedVariable { input } => {
                write!(f, "Unclosed variable expression starting at `{input}`")
            }
            PathError::MissingEnvVar { var, input } => {
                write!(f, "Environment variable `{var}` not set in `{input}`")
            }
        }
    }
}

impl Error for PathError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            PathError::FailedToGetCurrentDir { source } => Some(source),
            _ => None,
        }
    }
}

#[derive(Debug)]
pub enum FileSystemError {
    // File operations
    ReadFile {
        path: PathBuf,
        source: std::io::Error,
    },

    WriteFile {
        path: PathBuf,
        source: std::io::Error,
    },

    CreateFile {
        path: PathBuf,
        source: std::io::Error,
    },

    RemoveFile {
        path: PathBuf,
        source: std::io::Error,
    },

    // Directory operations
    ReadDirectory {
        path: PathBuf,
        source: std::io::Error,
    },

    CreateDirectory {
        path: PathBuf,
        source: std::io::Error,
    },

    RemoveDirectory {
        path: PathBuf,
        source: std::io::Error,
    },

    // Symlink operations
    CreateSymlink {
        from: PathBuf,
        target: PathBuf,
        source: std::io::Error,
    },

    RemoveSymlink {
        path: PathBuf,
        source: std::io::Error,
    },

    ReadSymlink {
        path: PathBuf,
        source: std::io::Error,
    },

    // Path validation
    NotFound {
        path: PathBuf,
    },

    NotADirectory {
        path: PathBuf,
    },

    NotAFile {
        path: PathBuf,
    },
}

impl fmt::Display for FileSystemError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FileSystemError::ReadFile { path, source } => {
                write!(f, "Failed to read file `{}`: {source}", path.display())
            }
            FileSystemError::WriteFile { path, source } => {
                write!(f, "Failed to write file `{}`: {source}", path.display())
            }
            FileSystemError::CreateFile { path, source } => {
                write!(f, "Failed to create file `{}`: {source}", path.display())
            }
            FileSystemError::RemoveFile { path, source } => {
                write!(f, "Failed to remove file `{}`: {source}", path.display())
            }
            FileSystemError::ReadDirectory { path, source } => {
                write!(f, "Failed to read directory `{}`: {source}", path.display())
            }
            FileSystemError::CreateDirectory { path, source } => {
                write!(
                    f,
                    "Failed to create directory `{}`: {source}",
                    path.display()
                )
            }
            FileSystemError::RemoveDirectory { path, source } => {
                write!(
                    f,
                    "Failed to remove directory `{}`: {source}",
                    path.display()
                )
            }
            FileSystemError::CreateSymlink {
                from,
                target,
                source,
            } => {
                write!(
                    f,
                    "Failed to create symlink from `{}` to `{}`: {source}",
                    from.display(),
                    target.display()
                )
            }
            FileSystemError::RemoveSymlink { path, source } => {
                write!(f, "Failed to remove symlink `{}`: {source}", path.display())
            }
            FileSystemError::ReadSymlink { path, source } => {
                write!(f, "Failed to read symlink `{}`: {source}", path.display())
            }
            FileSystemError::NotFound { path } => {
                write!(f, "Path `{}` not found", path.display())
            }
            FileSystemError::NotADirectory { path } => {
                write!(f, "`{}` is not a directory", path.display())
            }
            FileSystemError::NotAFile { path } => {
                write!(f, "`{}` is not a file", path.display())
            }
        }
    }
}

impl Error for FileSystemError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            FileSystemError::ReadFile { source, .. } => Some(source),
            FileSystemError::WriteFile { source, .. } => Some(source),
            FileSystemError::CreateFile { source, .. } => Some(source),
            FileSystemError::RemoveFile { source, .. } => Some(source),
            FileSystemError::ReadDirectory { source, .. } => Some(source),
            FileSystemError::CreateDirectory { source, .. } => Some(source),
            FileSystemError::RemoveDirectory { source, .. } => Some(source),
            FileSystemError::CreateSymlink { source, .. } => Some(source),
            FileSystemError::RemoveSymlink { source, .. } => Some(source),
            FileSystemError::ReadSymlink { source, .. } => Some(source),
            _ => None,
        }
    }
}

pub struct IoContext {
    path: PathBuf,
    operation: IoOperation,
}

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

#[derive(Debug)]
pub enum UtilsError {
    Bytes(BytesError),
    Path(PathError),
    FileSystem(FileSystemError),
}

impl fmt::Display for UtilsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            UtilsError::Bytes(err) => write!(f, "{err}"),
            UtilsError::Path(err) => write!(f, "{err}"),
            UtilsError::FileSystem(err) => write!(f, "{err}"),
        }
    }
}

impl Error for UtilsError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            UtilsError::Bytes(err) => Some(err),
            UtilsError::Path(err) => Some(err),
            UtilsError::FileSystem(err) => Some(err),
        }
    }
}

impl From<BytesError> for UtilsError {
    fn from(err: BytesError) -> Self {
        UtilsError::Bytes(err)
    }
}

impl From<PathError> for UtilsError {
    fn from(err: PathError) -> Self {
        UtilsError::Path(err)
    }
}

impl From<FileSystemError> for UtilsError {
    fn from(err: FileSystemError) -> Self {
        UtilsError::FileSystem(err)
    }
}

pub type BytesResult<T> = std::result::Result<T, BytesError>;
pub type FileSystemResult<T> = std::result::Result<T, FileSystemError>;
pub type HashResult<T> = std::result::Result<T, HashError>;
pub type PathResult<T> = std::result::Result<T, PathError>;

pub type UtilsResult<T> = std::result::Result<T, UtilsError>;

#[cfg(test)]
mod tests {
    use super::*;
    use std::io;

    #[test]
    fn test_bytes_error_display() {
        let error = BytesError::ParseFailed {
            input: "test".to_string(),
            reason: "invalid".to_string(),
        };
        assert_eq!(
            error.to_string(),
            "Failed to parse `test` as bytes: invalid"
        );
    }

    #[test]
    fn test_hash_error_display_and_source() {
        let io_error = io::Error::new(io::ErrorKind::NotFound, "file not found");
        let error = HashError::ReadFailed {
            path: PathBuf::from("/test"),
            source: io_error,
        };
        assert_eq!(
            error.to_string(),
            "Failed to read file `/test`: file not found"
        );
        assert!(error.source().is_some());
    }

    #[test]
    fn test_path_error_display_and_source() {
        let io_error = io::Error::other("some error");
        let current_dir_error = PathError::FailedToGetCurrentDir { source: io_error };
        assert_eq!(
            current_dir_error.to_string(),
            "Failed to get current directory: some error"
        );
        assert!(current_dir_error.source().is_some());

        let empty_error = PathError::Empty;
        assert_eq!(empty_error.to_string(), "Path is empty");
        assert!(empty_error.source().is_none());

        let missing_env_var_error = PathError::MissingEnvVar {
            var: "VAR".to_string(),
            input: "$VAR".to_string(),
        };
        assert_eq!(
            missing_env_var_error.to_string(),
            "Environment variable `VAR` not set in `$VAR`"
        );
        assert!(missing_env_var_error.source().is_none());

        let unclosed_variable_error = PathError::UnclosedVariable {
            input: "${VAR".to_string(),
        };
        assert_eq!(
            unclosed_variable_error.to_string(),
            "Unclosed variable expression starting at `${VAR`"
        );
        assert!(unclosed_variable_error.source().is_none());
    }

    #[test]
    fn test_file_system_error_display_and_source() {
        let io_error = io::Error::new(io::ErrorKind::PermissionDenied, "permission denied");
        let file_error = FileSystemError::ReadFile {
            path: PathBuf::from("/file"),
            source: io_error,
        };
        assert_eq!(
            file_error.to_string(),
            "Failed to read file `/file`: permission denied"
        );
        assert!(file_error.source().is_some());

        let io_error2 = io::Error::new(io::ErrorKind::PermissionDenied, "permission denied");
        let dir_error = FileSystemError::CreateDirectory {
            path: PathBuf::from("/dir"),
            source: io_error2,
        };
        assert_eq!(
            dir_error.to_string(),
            "Failed to create directory `/dir`: permission denied"
        );
        assert!(dir_error.source().is_some());

        let not_a_dir_error = FileSystemError::NotADirectory {
            path: PathBuf::from("/path"),
        };
        assert_eq!(not_a_dir_error.to_string(), "`/path` is not a directory");
        assert!(not_a_dir_error.source().is_none());
    }

    #[test]
    fn test_utils_error_display_and_source_and_from() {
        let bytes_error = BytesError::ParseFailed {
            input: "test".to_string(),
            reason: "invalid".to_string(),
        };
        let utils_error_from_bytes = UtilsError::from(bytes_error);
        assert_eq!(
            utils_error_from_bytes.to_string(),
            "Failed to parse `test` as bytes: invalid"
        );
        assert!(utils_error_from_bytes.source().is_some());

        let path_error = PathError::Empty;
        let utils_error_from_path = UtilsError::from(path_error);
        assert_eq!(utils_error_from_path.to_string(), "Path is empty");
        assert!(utils_error_from_path.source().is_some());

        let fs_error = FileSystemError::NotADirectory {
            path: PathBuf::from("/path"),
        };
        let utils_error_from_fs = UtilsError::from(fs_error);
        assert_eq!(
            utils_error_from_fs.to_string(),
            "`/path` is not a directory"
        );
        assert!(utils_error_from_fs.source().is_some());
    }
}
