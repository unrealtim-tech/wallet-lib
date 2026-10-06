use bdk_wallet::chain::spk_client::SyncRequest;
use bdk_wallet::{KeychainKind, Update, Wallet as BdkWallet};

use crate::error::{Result, WalletError};

pub(crate) fn request(inner: &BdkWallet) -> SyncRequest<(KeychainKind, u32)> {
    inner.start_sync_with_revealed_spks().build()
}

pub(crate) fn apply(inner: &mut BdkWallet, update: Update) -> Result<()> {
    inner
        .apply_update(update)
        .map_err(|e| WalletError::ApplyUpdate(e.to_string()))
}
