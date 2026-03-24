use thiserror::Error;

use crate::declarative::DeclarativeError;

#[derive(Debug, Error)]
pub enum FomodError {
    #[error("XML parsing error: {0}")]
    Xml(#[from] quick_xml::DeError),

    #[error("missing required element: {0}")]
    MissingElement(&'static str),

    #[error("invalid attribute value for `{attr}`: {value}")]
    InvalidAttribute { attr: &'static str, value: String },

    #[error("unsupported schema version: {0}")]
    UnsupportedVersion(String),

    #[error(transparent)]
    Declarative(#[from] DeclarativeError),
}

pub type Result<T> = std::result::Result<T, FomodError>;
