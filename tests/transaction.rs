mod common;

use bitcoin::{Amount, Network};
use common::{OTHER, funded_wallet, recipient};
use wallet_lib::Wallet;
use wallet_lib::error::WalletError;
use wallet_lib::transaction::Recipient;
use wallet_lib::types::FeeRate;

fn fee_rate() -> FeeRate {
    FeeRate::from_sat_per_vb(2).unwrap()
}

#[test]
fn build_tx_pays_recipient_and_returns_change() {
    let mut wallet = funded_wallet(100_000);
    let to = recipient(40_000);

    let psbt = wallet.build_tx(&[to.clone()], fee_rate()).unwrap();
    assert_eq!(psbt.unsigned_tx.input.len(), 1);
    assert_eq!(psbt.unsigned_tx.output.len(), 2, "recipient + change");
    assert!(psbt.unsigned_tx.output.iter().any(|o| o.script_pubkey == to.address.script_pubkey()));
}

#[test]
fn build_tx_reports_insufficient_funds() {
    let mut wallet = funded_wallet(10_000);
    let err = wallet.build_tx(&[recipient(50_000)], fee_rate()).unwrap_err();
    assert!(matches!(err, WalletError::InsufficientFunds { available: 10_000, .. }));
}

#[test]
fn build_tx_rejects_wrong_network_address() {
    let mut wallet = funded_wallet(100_000);
    let mainnet_addr = Wallet::from_mnemonic(OTHER, "", Network::Bitcoin).unwrap().new_address();
    let r = Recipient { address: mainnet_addr, amount: Amount::from_sat(1_000) };
    let err = wallet.build_tx(&[r], fee_rate()).unwrap_err();
    assert!(matches!(err, WalletError::InvalidAddress(_)));
}
