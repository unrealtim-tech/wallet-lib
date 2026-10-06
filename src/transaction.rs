use bdk_wallet::bitcoin::bip32::Xpriv;
use bdk_wallet::error::CreateTxError;
use bdk_wallet::{SignOptions, Wallet as BdkWallet};
use bitcoin::secp256k1::{All, Secp256k1};
use bitcoin::{Address, Amount, Network, Psbt, Transaction};

use crate::address;
use crate::error::{Result, WalletError};
use crate::types::FeeRate;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Recipient {
    pub address: Address,
    pub amount: Amount,
}

pub(crate) fn build(
    inner: &mut BdkWallet,
    network: Network,
    recipients: &[Recipient],
    fee_rate: FeeRate,
) -> Result<Psbt> {
    for r in recipients {
        address::ensure_network(&r.address, network)?;
    }

    let mut builder = inner.build_tx();
    for r in recipients {
        builder.add_recipient(r.address.script_pubkey(), r.amount);
    }
    builder.fee_rate(fee_rate.into_inner());
    builder.finish().map_err(|e| match e {
        CreateTxError::CoinSelection(f) => WalletError::InsufficientFunds {
            needed: f.needed.to_sat(),
            available: f.available.to_sat(),
        },
        other => WalletError::CreateTx(other.to_string()),
    })
}

pub(crate) fn sign(
    inner: &BdkWallet,
    keys: &[Xpriv],
    secp: &Secp256k1<All>,
    mut psbt: Psbt,
) -> Result<Transaction> {
    for key in keys {
        psbt.sign(key, secp)
            .map_err(|(_, errors)| WalletError::Signing(format!("{errors:?}")))?;
    }
    let finalized = inner
        .finalize_psbt(&mut psbt, SignOptions::default())
        .map_err(|e| WalletError::Signing(e.to_string()))?;
    if !finalized {
        return Err(WalletError::Signing("psbt could not be finalized".into()));
    }
    psbt.extract_tx()
        .map_err(|e| WalletError::Signing(e.to_string()))
}
