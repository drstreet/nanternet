use serde::{Serialize, Serializer};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{0}")]
    Message(String),
    #[error("filesystem error: {0}")]
    Io(#[from] std::io::Error),
    #[error("malformed data: {0}")]
    Json(#[from] serde_json::Error),
    #[error("request failed: {0}")]
    Http(#[from] reqwest::Error),
    #[error("ssh transport error: {0}")]
    Ssh(#[from] russh::Error),
    #[error("ssh key error: {0}")]
    SshKey(#[from] russh::keys::Error),
    #[error("credential store error: {0}")]
    Keychain(#[from] keyring::Error),
    #[error("tls error: {0}")]
    Tls(#[from] rustls::Error),
    #[error("{0}")]
    Tauri(#[from] tauri::Error),
}

pub type Result<T> = std::result::Result<T, Error>;

impl Error {
    pub fn msg(text: impl Into<String>) -> Self {
        Self::Message(text.into())
    }
}

impl From<tokio::time::error::Elapsed> for Error {
    fn from(_: tokio::time::error::Elapsed) -> Self {
        Self::Message("operation timed out".into())
    }
}

impl From<tokio::task::JoinError> for Error {
    fn from(value: tokio::task::JoinError) -> Self {
        Self::Message(value.to_string())
    }
}

impl Serialize for Error {
    fn serialize<S: Serializer>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_string())
    }
}
