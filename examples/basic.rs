//! End-to-end tour of the `wallet-lib` API using the in-memory mock backend.
//!
//! Run with: `cargo run --example basic`
//!
//! Steps: create a wallet, receive funds (simulated by a hand-built chain update),
//! sync through a `ChainBackend`, inspect balance and UTXOs, build a PSBT, sign it,
//! broadcast it, and handle errors.

use std::sync::Arc;

use bdk_wallet::Update;
use bitcoin::hashes::Hash;
use bitcoin::{Amount, Network, OutPoint, Transaction, TxIn, TxOut, Txid, absolute, transaction};
use wallet_lib::backend::mock::MockChainBackend;
use wallet_lib::error::WalletError;
use wallet_lib::transaction::Recipient;
use wallet_lib::types::FeeRate;
use wallet_lib::{ChainBackend, Wallet};

const MNEMONIC: &str =
    "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about";
const FRIEND: &str = "legal winner thank year wave sausage worth useful legal winner thank yellow";

/// Fabricates a chain update in which `address` receives `sats`. A real backend
/// (Esplora, Electrum, ...) would produce this from the blockchain.
fn fake_deposit(address: &bitcoin::Address, sats: u64) -> Update {
    let tx = Transaction {
        version: transaction::Version::TWO,
        lock_time: absolute::LockTime::ZERO,
        input: vec![TxIn {
            previous_output: OutPoint::new(Txid::from_byte_array([7; 32]), 0),
            ..Default::default()
        }],
        output: vec![TxOut {
            value: Amount::from_sat(sats),
            script_pubkey: address.script_pubkey(),
        }],
    };
    let mut update = Update::default();
    update.tx_update.seen_ats.insert((tx.compute_txid(), 1_700_000_000));
    update.tx_update.txs.push(Arc::new(tx));
    update
}

fn main() -> Result<(), WalletError> {
    // 1. Create a wallet. No backend is involved at construction time.
    //    (Alternatively: `Wallet::from_descriptor(external, Some(internal), network)`.)
    let mut wallet = Wallet::from_mnemonic(MNEMONIC, "", Network::Regtest)?;

    // 2. Reveal a receive address.
    let address = wallet.new_address();
    println!("receive address : {address}");

    // 3. Sync. The wallet asks (`sync_request`), the backend answers (`fetch`),
    //    and the wallet merges the answer (`apply_update`). `sync` does all three.
    let backend = MockChainBackend::with_update(fake_deposit(&address, 100_000));
    wallet.sync(&backend)?;

    // 4. Inspect state.
    let balance = wallet.balance();
    println!("total           : {}", balance.total());
    println!("spendable       : {} (deposit is unconfirmed)", balance.spendable());
    for utxo in wallet.list_utxos() {
        println!("utxo            : {} {} confirmed={}", utxo.outpoint, utxo.amount, utxo.confirmed);
    }

    // 5. Build a payment. Unconfirmed foreign coins are still selectable by BDK's
    //    default policy, so this succeeds; check `spendable()` in a real UI.
    let mut friend_wallet = Wallet::from_mnemonic(FRIEND, "", Network::Regtest)?;
    let friend = friend_wallet.new_address();
    let recipient = Recipient { address: friend, amount: Amount::from_sat(40_000) };
    let fee_rate = FeeRate::from_sat_per_vb(2).expect("valid fee rate");
    let psbt = wallet.build_tx(&[recipient], fee_rate)?;
    println!("psbt            : {} inputs, {} outputs", psbt.unsigned_tx.input.len(), psbt.unsigned_tx.output.len());

    // 6. Sign (offline; no backend needed) and broadcast through a backend.
    let tx = wallet.sign(psbt)?;
    wallet.broadcast(&backend, &tx)?;
    println!("broadcast       : {}", tx.compute_txid());
    println!("backend saw     : {} tx", backend.broadcasted().len());

    // 7. Errors are typed.
    let too_much = Recipient {
        address: wallet.new_address(),
        amount: Amount::from_sat(10_000_000),
    };
    match wallet.build_tx(&[too_much], fee_rate) {
        Err(WalletError::InsufficientFunds { needed, available }) => {
            println!("insufficient    : need {needed}, have {available}");
        }
        other => println!("unexpected: {other:?}"),
    }

    // 8. Backends are interchangeable via trait objects.
    let offline: Box<dyn ChainBackend> = Box::new(MockChainBackend::unavailable("node down"));
    if let Err(e) = wallet.sync(offline.as_ref()) {
        println!("sync failed     : {e}");
    }
    Ok(())
}
