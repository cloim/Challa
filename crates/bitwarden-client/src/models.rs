use serde::{Deserialize, Serialize};
use zeroize::Zeroizing;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum BitwardenField {
    Username,
    Password,
    Totp,
    Notes,
    Custom(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum VaultStatus {
    Unavailable,
    Unauthenticated,
    Locked,
    Unlocked,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VaultItemSummary {
    pub id: String,
    pub name: String,
    pub username: Option<String>,
    pub fields: Vec<BitwardenField>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct VaultReference {
    pub item_id: String,
    pub field: BitwardenField,
}

#[derive(Debug, Serialize)]
pub struct VaultInfo {
    pub status: VaultStatus,
    pub cli_available: bool,
    pub cached_fields: usize,
    pub auto_lock_minutes: u64,
}

/// Debug and IPC never reveal field contents. The allocation is wiped on drop.
pub struct SecretFieldValue(Zeroizing<String>);
impl SecretFieldValue {
    pub fn new(value: String) -> Self {
        Self(Zeroizing::new(value))
    }
    pub fn expose_secret(&self) -> &str {
        &self.0
    }
    pub fn into_secret(self) -> Zeroizing<String> {
        self.0
    }
}
impl std::fmt::Debug for SecretFieldValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("SecretFieldValue([REDACTED])")
    }
}
