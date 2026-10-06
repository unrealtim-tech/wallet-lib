use std::collections::HashSet;
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};

use bdk_bitcoind_rpc::bitcoincore_rpc::{Auth, Client, RpcApi};
use bdk_bitcoind_rpc::{Emitter, NO_EXPECTED_MEMPOOL_TXS};
use bdk_wallet::chain::spk_client::SyncRequest;
use bdk_wallet::chain::{BlockId, CheckPoint, ConfirmationBlockTime, TxUpdate};
use bdk_wallet::{KeychainKind, Update};
use bitcoin::{Address, OutPoint, ScriptBuf, Transaction};
use wallet_lib::ChainBackend;
use wallet_lib::error::BackendError;

/// Block-scan progress of the sync in flight, readable from the UI thread.
#[derive(Default)]
pub struct Progress {
    pub scanned: AtomicU32,
    pub total: AtomicU32,
}

pub struct RpcBackend {
    client: Arc<Client>,
    url: String,
    progress: Arc<Progress>,
}

fn unavailable(e: impl std::fmt::Display) -> BackendError {
    BackendError::Unavailable(e.to_string())
}

impl RpcBackend {
    pub fn new(url: &str, user: &str, pass: &str) -> Result<Self, BackendError> {
        let client = Client::new(url, Auth::UserPass(user.to_owned(), pass.to_owned()))
            .map_err(unavailable)?;
        Ok(Self {
            client: Arc::new(client),
            url: url.to_owned(),
            progress: Arc::default(),
        })
    }

    pub fn url(&self) -> &str {
        &self.url
    }

    pub fn progress(&self) -> Arc<Progress> {
        Arc::clone(&self.progress)
    }

    /// Regtest only: mines `n` blocks paying the coinbase to `address`.
    pub fn mine(&self, n: u64, address: &Address) -> Result<usize, BackendError> {
        self.client
            .generate_to_address(n, address)
            .map(|hashes| hashes.len())
            .map_err(unavailable)
    }
}

/// A tx matters if it pays one of our scripts or spends an output we already know about.
fn relevant(tx: &Transaction, spks: &HashSet<ScriptBuf>, owned: &mut HashSet<OutPoint>) -> bool {
    let spends = tx.input.iter().any(|i| owned.contains(&i.previous_output));
    let pays = tx.output.iter().any(|o| spks.contains(&o.script_pubkey));
    if pays {
        let txid = tx.compute_txid();
        for (vout, o) in tx.output.iter().enumerate() {
            if spks.contains(&o.script_pubkey) {
                owned.insert(OutPoint::new(txid, vout as u32));
            }
        }
    }
    spends || pays
}

impl ChainBackend for RpcBackend {
    /// Regtest chains are tiny, so every sync rescans from genesis. That keeps spend detection
    /// simple and means the wallet's existing chain always connects to the update.
    fn fetch(&self, mut request: SyncRequest<(KeychainKind, u32)>) -> Result<Update, BackendError> {
        let spks: HashSet<ScriptBuf> = request
            .iter_spks_with_expected_txids()
            .map(|s| s.spk)
            .collect();
        let mut owned: HashSet<OutPoint> = HashSet::new();

        let genesis = CheckPoint::new(BlockId {
            height: 0,
            hash: self.client.get_block_hash(0).map_err(unavailable)?,
        });
        let total = self.client.get_block_count().map_err(unavailable)? as u32 + 1;
        self.progress.scanned.store(0, Ordering::Relaxed);
        self.progress.total.store(total, Ordering::Relaxed);
        let mut emitter = Emitter::new(
            self.client.clone(),
            genesis.clone(),
            0,
            NO_EXPECTED_MEMPOOL_TXS,
        );

        let mut tx_update = TxUpdate::<ConfirmationBlockTime>::default();
        let mut tip = genesis;
        while let Some(ev) = emitter.next_block().map_err(unavailable)? {
            let anchor = ConfirmationBlockTime {
                block_id: ev.checkpoint.block_id(),
                confirmation_time: ev.block.header.time as u64,
            };
            for tx in &ev.block.txdata {
                if relevant(tx, &spks, &mut owned) {
                    tx_update.anchors.insert((anchor, tx.compute_txid()));
                    tx_update.txs.push(Arc::new(tx.clone()));
                }
            }
            self.progress.scanned.store(ev.block_height() + 1, Ordering::Relaxed);
            tip = ev.checkpoint;
        }

        let mempool = emitter.mempool().map_err(unavailable)?;
        for (tx, seen_at) in mempool.update {
            if relevant(&tx, &spks, &mut owned) {
                tx_update.seen_ats.insert((tx.compute_txid(), seen_at));
                tx_update.txs.push(tx);
            }
        }
        tx_update.evicted_ats.extend(mempool.evicted);

        Ok(Update {
            last_active_indices: Default::default(),
            tx_update,
            chain: Some(tip),
        })
    }

    fn broadcast(&self, tx: &Transaction) -> Result<(), BackendError> {
        self.client
            .send_raw_transaction(tx)
            .map(|_| ())
            .map_err(|e| BackendError::BroadcastRejected(e.to_string()))
    }
}
