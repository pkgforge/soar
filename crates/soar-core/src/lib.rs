use error::SoarError;

pub mod constants;
pub mod database;
pub mod error;
pub mod package;
pub mod utils;

pub type SoarResult<T> = std::result::Result<T, SoarError>;
