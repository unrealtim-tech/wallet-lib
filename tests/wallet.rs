mod common;

use bitcoin::Amount;
use common::{funded_wallet, funding_update, wallet};
use wallet_lib::types::Balance;

#[test]
fn new_wallet_has_zero_balance() {
    assert_eq!(wallet().balance(), Balance::default());
}

#[test]
fn sync_request_covers_revealed_addresses() {
    let mut wallet = wallet();
    wallet.new_address();
    wallet.new_address();
    assert_eq!(wallet.sync_request().progress().spks_remaining, 2);
}

#[test]
fn apply_update_changes_balance_without_any_backend() {
    let wallet = funded_wallet(100_000);
    assert_eq!(wallet.balance().total(), Amount::from_sat(100_000));
}

#[test]
fn balance_separates_pending_from_spendable() {
    let b = funded_wallet(100_000).balance();
    assert_eq!(b.untrusted_pending, Amount::from_sat(100_000));
    assert_eq!(b.confirmed, Amount::ZERO);
    assert_eq!(b.spendable(), Amount::ZERO);
    assert_eq!(b.total(), Amount::from_sat(100_000));
}

#[test]
fn list_utxos_returns_synced_outputs() {
    let mut wallet = wallet();
    assert!(wallet.list_utxos().is_empty());

    for sats in [100_000, 50_000] {
        let update = funding_update(&mut wallet, sats);
        wallet.apply_update(update).unwrap();
    }

    let utxos = wallet.list_utxos();
    let mut amounts: Vec<u64> = utxos.iter().map(|u| u.amount.to_sat()).collect();
    amounts.sort();
    assert_eq!(amounts, vec![50_000, 100_000]);
    assert!(utxos.iter().all(|u| !u.confirmed && !u.is_change));
    assert_eq!(wallet.balance().total(), Amount::from_sat(150_000));
}
