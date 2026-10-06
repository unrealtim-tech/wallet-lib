mod common;

use bdk_wallet::Update;
use common::{funded_wallet, recipient};
use wallet_lib::ChainBackend;
use wallet_lib::backend::mock::MockChainBackend;
use wallet_lib::types::FeeRate;

#[test]
fn signed_tx_has_witness_and_broadcasts() {
    let mut wallet = funded_wallet(100_000);
    let psbt = wallet
        .build_tx(&[recipient(40_000)], FeeRate::from_sat_per_vb(2).unwrap())
        .unwrap();

    let tx = wallet.sign(psbt).unwrap();
    assert!(!tx.input[0].witness.is_empty(), "input must be signed");

    let backend = MockChainBackend::with_update(Update::default());
    backend.broadcast(&tx).unwrap();
    assert_eq!(backend.broadcasted(), vec![tx]);
}
