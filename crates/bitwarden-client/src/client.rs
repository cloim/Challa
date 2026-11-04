use crate::error::BitwardenError;
use crate::models::{BitwardenField, SecretFieldValue, VaultItemSummary, VaultStatus};
use std::sync::RwLock;

pub struct BitwardenClient {
    status: RwLock<VaultStatus>,
}

impl BitwardenClient {
    pub fn new() -> Self {
        Self {
            status: RwLock::new(VaultStatus::Locked),
        }
    }

    pub fn status(&self) -> VaultStatus {
        self.status.read().map(|s| s.clone()).unwrap_or(VaultStatus::Error("Lock poisoned".into()))
    }

    pub fn lock(&self) {
        if let Ok(mut status) = self.status.write() {
            *status = VaultStatus::Locked;
        }
    }

    pub fn list_items(&self) -> Result<Vec<VaultItemSummary>, BitwardenError> {
        if self.status() == VaultStatus::Locked {
            return Err(BitwardenError::VaultLocked);
        }
        Ok(Vec::new())
    }

    pub fn get_field(
        &self,
        _item_id: &str,
        _field: &BitwardenField,
    ) -> Result<SecretFieldValue, BitwardenError> {
        if self.status() == VaultStatus::Locked {
            return Err(BitwardenError::VaultLocked);
        }
        Err(BitwardenError::ItemNotFound(_item_id.to_string()))
    }
}

impl Default for BitwardenClient {
    fn default() -> Self {
        Self::new()
    }
}
