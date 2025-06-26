use error::SoarError;

pub mod config;
pub mod constants;
pub mod database;
pub mod error;
pub mod metadata;
pub mod package;
pub mod repositories;
pub mod toml;
pub mod utils;

pub type SoarResult<T> = std::result::Result<T, SoarError>;
