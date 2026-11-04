use secrecy::SecretString;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum BitwardenField {
    Username,
    Password,
    Totp,
    Custom(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum VaultStatus {
    Locked,
    Unlocked,
    Syncing,
    Error(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VaultItemSummary {
    pub id: String,
    pub name: String,
    pub username: Option<String>,
}

pub struct SecretFieldValue {
    secret: SecretString,
}

impl SecretFieldValue {
    pub fn new(val: String) -> Self {
        Self {
            secret: SecretString::from(val),
        }
    }

    pub fn expose_secret(&self) -> &str {
        use secrecy::ExposeSecret;
        self.secret.expose_secret()
    }
}
