use bdk_wallet::{KeychainKind, LocalOutput, Wallet as BdkWallet};
use bitcoin::{Amount, OutPoint};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Utxo {
    pub outpoint: OutPoint,
    pub amount: Amount,
    pub is_change: bool,
    pub confirmed: bool,
}

impl From<LocalOutput> for Utxo {
    fn from(o: LocalOutput) -> Self {
        Self {
            outpoint: o.outpoint,
            amount: o.txout.value,
            is_change: o.keychain == KeychainKind::Internal,
            confirmed: o.chain_position.is_confirmed(),
        }
    }
}

pub(crate) fn list(inner: &BdkWallet) -> Vec<Utxo> {
    let mut utxos: Vec<Utxo> = inner.list_unspent().map(Utxo::from).collect();
    utxos.sort_by_key(|u| u.outpoint);
    utxos
}
