use bdk_wallet::bitcoin::bip32::Xpriv;
use bdk_wallet::keys::bip39::Mnemonic;
use bitcoin::Network;

use crate::error::{Result, WalletError};

pub(crate) struct Keys {
    pub external_descriptor: String,
    pub internal_descriptor: String,
}

pub(crate) fn from_mnemonic(mnemonic: &str, passphrase: &str, network: Network) -> Result<Keys> {
    let mnemonic =
        Mnemonic::parse(mnemonic).map_err(|e| WalletError::InvalidMnemonic(e.to_string()))?;
    let seed = mnemonic.to_seed(passphrase);
    let master =
        Xpriv::new_master(network, &seed).map_err(|e| WalletError::Create(e.to_string()))?;

    let coin_type = if network == Network::Bitcoin { 0 } else { 1 };
    Ok(Keys {
        external_descriptor: format!("wpkh({master}/84h/{coin_type}h/0h/0/*)"),
        internal_descriptor: format!("wpkh({master}/84h/{coin_type}h/0h/1/*)"),
    })
}
