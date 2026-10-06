mod common;

use bdk_wallet::Update;
use bdk_wallet::keys::bip39::Mnemonic;
use bitcoin::bip32::{DerivationPath, Xpriv, Xpub};
use bitcoin::secp256k1::Secp256k1;
use bitcoin::Network;
use common::{ABANDON, funding_update, recipient};
use wallet_lib::Wallet;
use wallet_lib::backend::mock::MockChainBackend;
use wallet_lib::error::{BackendError, WalletError};
use wallet_lib::types::FeeRate;

fn master() -> Xpriv {
    let seed = Mnemonic::parse(ABANDON).unwrap().to_seed("");
    Xpriv::new_master(Network::Regtest, &seed).unwrap()
}

fn descriptors() -> (String, String) {
    let m = master();
    (format!("wpkh({m}/84h/1h/0h/0/*)"), format!("wpkh({m}/84h/1h/0h/1/*)"))
}

#[test]
fn descriptor_wallet_matches_mnemonic_wallet() {
    let (ext, int) = descriptors();
    let mut from_desc = Wallet::from_descriptor(&ext, Some(&int), Network::Regtest).unwrap();
    let mut from_mnemonic = common::wallet();
    assert_eq!(from_desc.new_address(), from_mnemonic.new_address());
    assert_eq!(from_desc.network(), Network::Regtest);
}

#[test]
fn single_descriptor_wallet_can_receive() {
    let (ext, _) = descriptors();
    let mut wallet = Wallet::from_descriptor(&ext, None, Network::Regtest).unwrap();
    wallet.new_address();
}

#[test]
fn invalid_descriptor_is_rejected() {
    let err = Wallet::from_descriptor("nonsense", None, Network::Regtest).err();
    assert!(matches!(err, Some(WalletError::Create(_))));
}

#[test]
fn watch_only_descriptor_cannot_sign() {
    let secp = Secp256k1::new();
    let master = master();
    let path: DerivationPath = "m/84h/1h/0h".parse().unwrap();
    let xpub = Xpub::from_priv(&secp, &master.derive_priv(&secp, &path).unwrap());
    let fp = master.fingerprint(&secp);
    let ext = format!("wpkh([{fp}/84h/1h/0h]{xpub}/0/*)");
    let mut wallet = Wallet::from_descriptor(&ext, None, Network::Regtest).unwrap();
    let update = funding_update(&mut wallet, 100_000);
    wallet.apply_update(update).unwrap();

    let psbt = wallet
        .build_tx(&[recipient(10_000)], FeeRate::from_sat_per_vb(2).unwrap())
        .unwrap();
    assert!(matches!(wallet.sign(psbt), Err(WalletError::Signing(_))));
}

#[test]
fn wallet_broadcast_goes_through_backend() {
    let mut wallet = common::funded_wallet(100_000);
    let psbt = wallet
        .build_tx(&[recipient(40_000)], FeeRate::from_sat_per_vb(2).unwrap())
        .unwrap();
    let tx = wallet.sign(psbt).unwrap();

    let backend = MockChainBackend::with_update(Update::default());
    wallet.broadcast(&backend, &tx).unwrap();
    assert_eq!(backend.broadcasted(), vec![tx.clone()]);

    let down = MockChainBackend::unavailable("offline");
    assert!(matches!(
        wallet.broadcast(&down, &tx),
        Err(WalletError::Backend(BackendError::Unavailable(_)))
    ));
}
