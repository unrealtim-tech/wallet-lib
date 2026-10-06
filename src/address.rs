use bdk_wallet::{KeychainKind, Wallet as BdkWallet};
use bitcoin::{Address, Network};

use crate::error::{Result, WalletError};

pub(crate) fn next_receive(inner: &mut BdkWallet) -> Address {
    inner.reveal_next_address(KeychainKind::External).address
}

pub(crate) fn ensure_network(address: &Address, network: Network) -> Result<()> {
    if address.as_unchecked().is_valid_for_network(network) {
        Ok(())
    } else {
        Err(WalletError::InvalidAddress(format!(
            "{address} is not valid for {network}"
        )))
    }
}
