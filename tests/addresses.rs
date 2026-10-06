mod common;

use bitcoin::Network;
use common::ABANDON;
use wallet_lib::Wallet;
use wallet_lib::error::WalletError;

#[test]
fn first_address_matches_bip84_test_vector() {
    let mut wallet = Wallet::from_mnemonic(ABANDON, "", Network::Bitcoin).unwrap();
    assert_eq!(
        wallet.new_address().to_string(),
        "bc1qcr8te4kr609gcawutmrza0j4xv80jy8z306fyu"
    );
}

#[test]
fn addresses_advance() {
    let mut wallet = common::wallet();
    let a = wallet.new_address();
    let b = wallet.new_address();
    assert_ne!(a, b);
}

#[test]
fn invalid_mnemonic_is_rejected() {
    let err = Wallet::from_mnemonic("not a real mnemonic", "", Network::Bitcoin).err();
    assert!(matches!(err, Some(WalletError::InvalidMnemonic(_))));
}
