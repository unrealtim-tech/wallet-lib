//! An in-memory backend for tests and offline demos.

use std::sync::Mutex;

use bdk_wallet::chain::spk_client::SyncRequest;
use bdk_wallet::{KeychainKind, Update};
use bitcoin::Transaction;

use crate::backend::ChainBackend;
use crate::error::BackendError;

enum Mode {
    Serving(Update),
    Unavailable(String),
}

/// A [`ChainBackend`] serving a fixed [`Update`] and recording broadcasts.
pub struct MockChainBackend {
    mode: Mode,
    broadcasted: Mutex<Vec<Transaction>>,
}

impl MockChainBackend {
    /// Creates a backend whose `fetch` always returns a clone of `update`.
    pub fn with_update(update: Update) -> Self {
        Self { mode: Mode::Serving(update), broadcasted: Mutex::default() }
    }

    /// Creates a backend whose every call fails with [`BackendError::Unavailable`].
    pub fn unavailable(reason: impl Into<String>) -> Self {
        Self { mode: Mode::Unavailable(reason.into()), broadcasted: Mutex::default() }
    }

    /// Transactions accepted by `broadcast` so far.
    pub fn broadcasted(&self) -> Vec<Transaction> {
        self.broadcasted.lock().unwrap().clone()
    }
}

impl ChainBackend for MockChainBackend {
    fn fetch(&self, _request: SyncRequest<(KeychainKind, u32)>) -> Result<Update, BackendError> {
        match &self.mode {
            Mode::Serving(update) => Ok(update.clone()),
            Mode::Unavailable(reason) => Err(BackendError::Unavailable(reason.clone())),
        }
    }

    fn broadcast(&self, tx: &Transaction) -> Result<(), BackendError> {
        match &self.mode {
            Mode::Serving(_) => {
                self.broadcasted.lock().unwrap().push(tx.clone());
                Ok(())
            }
            Mode::Unavailable(reason) => Err(BackendError::Unavailable(reason.clone())),
        }
    }
}
