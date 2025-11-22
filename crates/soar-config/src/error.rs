use miette::Diagnostic;
use soar_utils::error::{PathError, UtilsError};
use thiserror::Error;

#[derive(Error, Diagnostic, Debug)]
pub enum ConfigError {
    #[error("TOML serialization error: {0}")]
    #[diagnostic(
        code(soar_config::toml_serialize),
        help("Check your configuration structure for invalid values")
    )]
    TomlSerError(#[from] toml::ser::Error),

    #[error("TOML deserialization error: {0}")]
    #[diagnostic(
        code(soar_config::toml_deserialize),
        help("Check your config.toml syntax and structure")
    )]
    TomlDeError(#[from] toml::de::Error),

    #[error("Configuration file already exists")]
    #[diagnostic(
        code(soar_config::already_exists),
        help("Remove the existing config file or use a different location")
    )]
    ConfigAlreadyExists,

    #[error("Invalid profile: {0}")]
    #[diagnostic(
        code(soar_config::invalid_profile),
        help("Check available profiles in your config file")
    )]
    InvalidProfile(String),

    #[error("Missing default profile: {0}")]
    #[diagnostic(
        code(soar_config::missing_default_profile),
        help("Ensure the default_profile field references an existing profile")
    )]
    MissingDefaultProfile(String),

    #[error("Missing profile: {0}")]
    #[diagnostic(
        code(soar_config::missing_profile),
        help("Add the profile to your configuration or use an existing one")
    )]
    MissingProfile(String),

    #[error("Invalid repository name: {0}")]
    #[diagnostic(code(soar_config_invalid_repository))]
    InvalidRepository(String),

    #[error("Invalid repository URL: {0}")]
    #[diagnostic(code(soar_config_invalid_repository_url))]
    InvalidRepositoryUrl(String),

    #[error("Reserved repository name 'local' cannot be used")]
    #[diagnostic(
        code(soar_config::reserved_repo_name),
        help("Choose a different name for your repository")
    )]
    ReservedRepositoryName,

    #[error("Duplicate repository name: {0}")]
    #[diagnostic(
        code(soar_config::duplicate_repo),
        help("Each repository must have a unique name")
    )]
    DuplicateRepositoryName(String),

    #[error("Repository name cannot start with `nest-`")]
    #[diagnostic(
        code(soar_config::invalid_repository_name),
        help("Repository names cannot start with `nest-`")
    )]
    InvalidRepositoryNameStartsWithNest,

    #[error("IO error: {0}")]
    #[diagnostic(code(soar_config::io))]
    IoError(#[from] std::io::Error),

    #[error(transparent)]
    #[diagnostic(code(soar_config::utils))]
    Utils(#[from] soar_utils::error::UtilsError),

    #[error("Failed to parse TOML: {0}")]
    #[diagnostic(code(soar_config::toml))]
    Toml(#[from] toml_edit::TomlError),

    #[error("Encountered unexpected TOML item: {0}")]
    #[diagnostic(code(soar_config::unexpected_toml_item))]
    UnexpectedTomlItem(String),

    #[error("Failed to annotate first table in array: {0}")]
    #[diagnostic(code(soar_config::annotate_first_table))]
    AnnotateFirstTable(String),
}

impl From<PathError> for ConfigError {
    fn from(err: PathError) -> Self {
        Self::Utils(UtilsError::Path(err))
    }
}

impl From<soar_utils::error::BytesError> for ConfigError {
    fn from(err: soar_utils::error::BytesError) -> Self {
        Self::Utils(UtilsError::Bytes(err))
    }
}

impl From<soar_utils::error::FileSystemError> for ConfigError {
    fn from(err: soar_utils::error::FileSystemError) -> Self {
        Self::Utils(UtilsError::FileSystem(err))
    }
}

pub type Result<T> = std::result::Result<T, ConfigError>;
