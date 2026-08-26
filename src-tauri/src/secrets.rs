use keyring::{Entry, Error as KeyringError};

use crate::error::Result;

const SERVICE: &str = "IranNANternet";

pub const TELEGRAM_TOKEN: &str = "notifications.telegram-token";

pub fn target_key(id: &str) -> String {
    format!("target.{id}")
}

pub async fn store(key: &str, value: &str) -> Result<()> {
    let key = key.to_owned();
    let value = value.to_owned();
    blocking(move || Entry::new(SERVICE, &key)?.set_password(&value)).await
}

pub async fn read(key: &str) -> Result<Option<String>> {
    let key = key.to_owned();
    blocking(move || match Entry::new(SERVICE, &key)?.get_password() {
        Ok(secret) => Ok(Some(secret)),
        Err(KeyringError::NoEntry) => Ok(None),
        Err(err) => Err(err),
    })
    .await
}

pub async fn forget(key: &str) -> Result<()> {
    let key = key.to_owned();
    blocking(
        move || match Entry::new(SERVICE, &key)?.delete_credential() {
            Ok(()) | Err(KeyringError::NoEntry) => Ok(()),
            Err(err) => Err(err),
        },
    )
    .await
}

pub async fn exists(key: &str) -> bool {
    matches!(read(key).await, Ok(Some(_)))
}

async fn blocking<T, F>(task: F) -> Result<T>
where
    T: Send + 'static,
    F: FnOnce() -> std::result::Result<T, KeyringError> + Send + 'static,
{
    Ok(tokio::task::spawn_blocking(task).await??)
}
