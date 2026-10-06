#![allow(dead_code)]

use bdk_wallet::Update;
use bitcoin::hashes::Hash;
use bitcoin::{Amount, Network, OutPoint, Transaction, TxIn, TxOut, Txid, absolute, transaction};
use std::sync::Arc;
use wallet_lib::Wallet;
use wallet_lib::transaction::Recipient;

pub const ABANDON: &str =
    "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about";
pub const OTHER: &str =
    "legal winner thank year wave sausage worth useful legal winner thank yellow";

pub fn wallet() -> Wallet {
    Wallet::from_mnemonic(ABANDON, "", Network::Regtest).unwrap()
}

pub fn funding_update(wallet: &mut Wallet, sats: u64) -> Update {
    let addr = wallet.new_address();
    let tx = Transaction {
        version: transaction::Version::TWO,
        lock_time: absolute::LockTime::ZERO,
        input: vec![TxIn {
            previous_output: OutPoint::new(Txid::from_byte_array([sats as u8; 32]), 0),
            ..Default::default()
        }],
        output: vec![TxOut {
            value: Amount::from_sat(sats),
            script_pubkey: addr.script_pubkey(),
        }],
    };
    let mut update = Update::default();
    update
        .tx_update
        .seen_ats
        .insert((tx.compute_txid(), 1_700_000_000));
    update.tx_update.txs.push(Arc::new(tx));
    update
}

pub fn funded_wallet(sats: u64) -> Wallet {
    let mut wallet = wallet();
    let update = funding_update(&mut wallet, sats);
    wallet.apply_update(update).unwrap();
    wallet
}

pub fn recipient(sats: u64) -> Recipient {
    let mut other = Wallet::from_mnemonic(OTHER, "", Network::Regtest).unwrap();
    Recipient {
        address: other.new_address(),
        amount: Amount::from_sat(sats),
    }
}
