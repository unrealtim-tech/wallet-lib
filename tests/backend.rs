mod common;

use bdk_wallet::Update;
use bitcoin::{Amount, Transaction, absolute, transaction};
use common::{funding_update, wallet};
use wallet_lib::ChainBackend;
use wallet_lib::backend::mock::MockChainBackend;
use wallet_lib::error::{BackendError, WalletError};
use wallet_lib::types::Balance;

#[test]
fn sync_updates_balance_from_mock_backend() {
    let mut wallet = wallet();
    let backend = MockChainBackend::with_update(funding_update(&mut wallet, 50_000));
    wallet.sync(&backend).unwrap();
    assert_eq!(wallet.balance().total(), Amount::from_sat(50_000));
}

#[test]
fn sync_propagates_backend_errors_and_leaves_state_alone() {
    let mut wallet = wallet();
    let backend = MockChainBackend::unavailable("node down");
    let err = wallet.sync(&backend).unwrap_err();
    assert!(matches!(err, WalletError::Backend(BackendError::Unavailable(_))));
    assert_eq!(wallet.balance(), Balance::default());
}

#[test]
fn sync_works_through_a_trait_object() {
    let mut wallet = wallet();
    let backend: Box<dyn ChainBackend> =
        Box::new(MockChainBackend::with_update(funding_update(&mut wallet, 1_000)));
    wallet.sync(backend.as_ref()).unwrap();
    assert_eq!(wallet.balance().total(), Amount::from_sat(1_000));
}

#[test]
fn mock_records_broadcasts_and_can_fail() {
    let tx = Transaction {
        version: transaction::Version::TWO,
        lock_time: absolute::LockTime::ZERO,
        input: vec![],
        output: vec![],
    };
    let ok = MockChainBackend::with_update(Update::default());
    ok.broadcast(&tx).unwrap();
    assert_eq!(ok.broadcasted(), vec![tx.clone()]);

    let down = MockChainBackend::unavailable("offline");
    assert!(matches!(down.broadcast(&tx), Err(BackendError::Unavailable(_))));
}
