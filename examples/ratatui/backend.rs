//! Chain backend choices for the TUI: Esplora over HTTP, or the in-memory mock.

use bdk_esplora::EsploraExt;
use bdk_esplora::esplora_client::{self, BlockingClient};
use bdk_wallet::chain::spk_client::SyncRequest;
use bdk_wallet::{KeychainKind, Update};
use bitcoin::Transaction;
use wallet_lib::ChainBackend;
use wallet_lib::error::BackendError;

const PARALLEL_REQUESTS: usize = 5;

/// A [`ChainBackend`] backed by an Esplora HTTP server (blocking client).
///
/// This is the pattern for plugging any chain source into `wallet-lib`:
/// answer a `SyncRequest` with an `Update`, and push transactions out.
pub struct EsploraBackend {
    client: BlockingClient,
    url: String,
}

impl EsploraBackend {
    pub fn new(url: &str) -> Result<Self, BackendError> {
        let client = esplora_client::Builder::new(url)
            .build_blocking();
        Ok(Self { client, url: url.to_owned() })
    }

    pub fn url(&self) -> &str {
        &self.url
    }
}

impl ChainBackend for EsploraBackend {
    fn fetch(&self, request: SyncRequest<(KeychainKind, u32)>) -> Result<Update, BackendError> {
        let response = self
            .client
            .sync(request, PARALLEL_REQUESTS)
            .map_err(|e| BackendError::Unavailable(e.to_string()))?;
        Ok(Update {
            last_active_indices: Default::default(),
            tx_update: response.tx_update,
            chain: response.chain_update,
        })
    }

    fn broadcast(&self, tx: &Transaction) -> Result<(), BackendError> {
        self.client
            .broadcast(tx)
            .map_err(|e| BackendError::BroadcastRejected(e.to_string()))
    }
}
