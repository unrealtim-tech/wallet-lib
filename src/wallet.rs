use bdk_wallet::bitcoin::bip32::Xpriv;
use bdk_wallet::chain::spk_client::SyncRequest;
use bdk_wallet::miniscript::Descriptor;
use bdk_wallet::miniscript::descriptor::DescriptorSecretKey;
use bdk_wallet::{KeychainKind, Update, Wallet as BdkWallet};
use bitcoin::secp256k1::{All, Secp256k1};
use bitcoin::{Address, Network, Psbt, Transaction};

use crate::backend::ChainBackend;
use crate::error::{Result, WalletError};
use crate::transaction::Recipient;
use crate::types::{Balance, FeeRate};
use crate::utxo::Utxo;
use crate::{address, keys, sync, transaction, utxo};

pub struct Wallet {
    inner: BdkWallet,
    network: Network,
    keys: Vec<Xpriv>,
    secp: Secp256k1<All>,
}

impl Wallet {
    pub fn from_mnemonic(mnemonic: &str, passphrase: &str, network: Network) -> Result<Self> {
        let keys = keys::from_mnemonic(mnemonic, passphrase, network)?;
        let inner = BdkWallet::create(
            keys.external_descriptor.clone(),
            keys.internal_descriptor.clone(),
        )
        .network(network)
        .create_wallet_no_persist()
        .map_err(|e| WalletError::Create(e.to_string()))?;
        Self::finish(
            inner,
            network,
            &[&keys.external_descriptor, &keys.internal_descriptor],
        )
    }

    pub fn from_descriptor(
        external: &str,
        internal: Option<&str>,
        network: Network,
    ) -> Result<Self> {
        let inner = match internal {
            Some(internal) => BdkWallet::create(external.to_owned(), internal.to_owned()),
            None => BdkWallet::create_single(external.to_owned()),
        }
        .network(network)
        .create_wallet_no_persist()
        .map_err(|e| WalletError::Create(e.to_string()))?;
        let descriptors: Vec<&str> = std::iter::once(external).chain(internal).collect();
        Self::finish(inner, network, &descriptors)
    }

    fn finish(inner: BdkWallet, network: Network, descriptors: &[&str]) -> Result<Self> {
        let secp = Secp256k1::new();
        let mut keys: Vec<Xpriv> = Vec::new();
        for d in descriptors {
            let (_, map) = Descriptor::parse_descriptor(&secp, d)
                .map_err(|e| WalletError::Create(e.to_string()))?;
            for secret in map.into_values() {
                match secret {
                    DescriptorSecretKey::XPrv(k) => keys.push(k.xkey),
                    DescriptorSecretKey::MultiXPrv(k) => keys.push(k.xkey),
                    _ => {}
                }
            }
        }
        Ok(Self {
            inner,
            network,
            keys,
            secp,
        })
    }

    pub fn network(&self) -> Network {
        self.network
    }

    pub fn new_address(&mut self) -> Address {
        address::next_receive(&mut self.inner)
    }

    pub fn sync_request(&self) -> SyncRequest<(KeychainKind, u32)> {
        sync::request(&self.inner)
    }

    pub fn apply_update(&mut self, update: Update) -> Result<()> {
        sync::apply(&mut self.inner, update)
    }

    pub fn sync(&mut self, backend: &(impl ChainBackend + ?Sized)) -> Result<()> {
        let update = backend.fetch(self.sync_request())?;
        self.apply_update(update)
    }

    pub fn balance(&self) -> Balance {
        self.inner.balance().into()
    }

    pub fn list_utxos(&self) -> Vec<Utxo> {
        utxo::list(&self.inner)
    }

    pub fn build_tx(&mut self, recipients: &[Recipient], fee_rate: FeeRate) -> Result<Psbt> {
        transaction::build(&mut self.inner, self.network, recipients, fee_rate)
    }

    pub fn sign(&self, psbt: Psbt) -> Result<Transaction> {
        transaction::sign(&self.inner, &self.keys, &self.secp, psbt)
    }

    /// Broadcasts `tx` and records it in the wallet as unconfirmed, so its inputs count as spent
    /// and its change output is available to the next payment before the next sync.
    pub fn broadcast(
        &mut self,
        backend: &(impl ChainBackend + ?Sized),
        tx: &Transaction,
    ) -> Result<()> {
        backend.broadcast(tx)?;
        let seen_at = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_secs());
        self.inner.apply_unconfirmed_txs([(tx.clone(), seen_at)]);
        Ok(())
    }
}
