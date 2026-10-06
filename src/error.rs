use thiserror::Error;

#[derive(Debug, Error)]
pub enum BackendError {
    #[error("backend unavailable: {0}")]
    Unavailable(String),
    #[error("malformed chain data: {0}")]
    MalformedData(String),
    #[error("transaction not found: {0}")]
    TxNotFound(bitcoin::Txid),
    #[error("broadcast rejected: {0}")]
    BroadcastRejected(String),
}

#[derive(Debug, Error)]
pub enum WalletError {
    #[error("invalid mnemonic: {0}")]
    InvalidMnemonic(String),
    #[error("could not create wallet: {0}")]
    Create(String),
    #[error("invalid address: {0}")]
    InvalidAddress(String),
    #[error("insufficient funds: need {needed} sats, have {available} sats")]
    InsufficientFunds { needed: u64, available: u64 },
    #[error("could not build transaction: {0}")]
    CreateTx(String),
    #[error("signing failed: {0}")]
    Signing(String),
    #[error("could not apply update: {0}")]
    ApplyUpdate(String),
    #[error(transparent)]
    Backend(#[from] BackendError),
}

pub type Result<T> = std::result::Result<T, WalletError>;
