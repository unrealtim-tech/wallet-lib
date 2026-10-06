pub mod mock;

use bdk_wallet::chain::spk_client::SyncRequest;
use bdk_wallet::{KeychainKind, Update};
use bitcoin::Transaction;

use crate::error::BackendError;

pub trait ChainBackend {
    fn fetch(&self, request: SyncRequest<(KeychainKind, u32)>) -> Result<Update, BackendError>;
    fn broadcast(&self, tx: &Transaction) -> Result<(), BackendError>;
}
