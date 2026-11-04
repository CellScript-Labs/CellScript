//! Proposal errors do not depend on compiler diagnostics.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{0}")]
    Io(#[from] std::io::Error),
    #[error("{0}")]
    InvalidEvidence(String),
}

impl Error {
    pub(crate) fn invalid(message: impl Into<String>) -> Self {
        Self::InvalidEvidence(message.into())
    }
}

pub type Result<T> = std::result::Result<T, Error>;
