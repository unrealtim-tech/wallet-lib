# wallet-lib

A thin, backend-agnostic Bitcoin wallet API built on [Bitcoin Dev Kit (BDK)](https://bitcoindevkit.org/).

The crate is a library, not an application. It wraps `bdk_wallet` behind a small API so a CLI, TUI or GUI can
create wallets, derive addresses, sync, build, sign and broadcast transactions without re-implementing
wallet logic. BDK performs the key, descriptor, PSBT and coin-selection work.

> Wallet state is in memory only, and the crate has not been audited. Use test networks (signet, testnet, regtest).

## Installation

```toml
[dependencies]
wallet-lib = { path = "." }   # or the published version / git URL
bitcoin = "0.32"              # for Network, Amount, Address
```

Minimum supported Rust version: 1.85 (edition 2024).

## Core idea: the backend is decoupled from the wallet

A `Wallet` never owns a chain backend. The application holds both and connects them:

```text
wallet.sync_request()        Wallet:  "this is what I need to know"
        |
        v
backend.fetch(request)       Backend: "this is what the chain says"
        |
        v
wallet.apply_update(update)  Wallet:  merges the answer into its state
```

`wallet.sync(&backend)` runs all three steps. Because the wallet holds no backend it works offline (build and
sign without a network), one backend can serve many wallets, and tests can feed hand-built updates.

## Quick start

```rust
use bitcoin::{Amount, Network};
use wallet_lib::{ChainBackend, Wallet};
use wallet_lib::transaction::Recipient;
use wallet_lib::types::FeeRate;

fn pay(backend: &dyn ChainBackend, to: bitcoin::Address) -> Result<(), wallet_lib::error::WalletError> {
    let mut wallet = Wallet::from_mnemonic("abandon ... about", "", Network::Signet)?;
    let receive = wallet.new_address();          // fund this address, then:
    wallet.sync(backend)?;

    println!("spendable: {}", wallet.balance().spendable());
    let psbt = wallet.build_tx(
        &[Recipient { address: to, amount: Amount::from_sat(10_000) }],
        FeeRate::from_sat_per_vb(2).unwrap(),
    )?;
    let tx = wallet.sign(psbt)?;
    wallet.broadcast(backend, &tx)?;
    Ok(())
}
```

## API reference

### `Wallet` (`wallet_lib::Wallet`)

| Method | Purpose |
|---|---|
| `Wallet::from_mnemonic(mnemonic, passphrase, network)` | Create a BIP84 (`wpkh`) wallet from a BIP39 phrase. Coin type `0'` on mainnet, `1'` otherwise. Errors: `InvalidMnemonic`, `Create`. |
| `Wallet::from_descriptor(external, internal: Option<&str>, network)` | Create from output descriptors. With private keys the wallet can sign; with public keys only (watch-only), `sign` fails. `internal = None` creates a single-descriptor wallet. Error: `Create`. |
| `network()` | The network the wallet was created for. |
| `new_address()` | Reveal and return the next receive address. Revealed addresses are covered by later syncs. |
| `sync_request()` | Build the BDK `SyncRequest` describing what the wallet needs from the chain. |
| `apply_update(update)` | Merge a BDK `Update` into the wallet. Error: `ApplyUpdate`. |
| `sync(&backend)` | `backend.fetch(sync_request())` then `apply_update`. State is unchanged on error. Errors: `Backend`, `ApplyUpdate`. |
| `balance()` | `Balance` split into confirmed, trusted pending, untrusted pending and immature. |
| `list_utxos()` | Unspent outputs as `Vec<Utxo>`, sorted by outpoint. |
| `build_tx(&[Recipient], FeeRate)` | Build an unsigned PSBT with change back to the wallet. Errors: `InvalidAddress` (wrong network), `InsufficientFunds { needed, available }`, `CreateTx`. |
| `sign(psbt)` | Sign and finalize, returning a `Transaction`. Works offline. Error: `Signing` (watch-only, unfinalizable). |
| `broadcast(&backend, &tx)` | Send a signed transaction through the backend. Error: `Backend`. |

### `ChainBackend` (`wallet_lib::ChainBackend`)

```rust
pub trait ChainBackend {
    fn fetch(&self, request: SyncRequest<(KeychainKind, u32)>) -> Result<Update, BackendError>;
    fn broadcast(&self, tx: &Transaction) -> Result<(), BackendError>;
}
```

Implement it for Esplora, Electrum, bitcoind RPC, or anything else. It is object safe, so `&dyn ChainBackend` and
`Box<dyn ChainBackend>` work. `examples/ratatui/backend.rs` contains an Esplora implementation.

`backend::mock::MockChainBackend` is an in-memory implementation for tests and demos:
`with_update(update)` serves a fixed update and records broadcasts (`broadcasted()` returns them);
`unavailable(reason)` fails every call with `BackendError::Unavailable`.

### Types

| Item | Description |
|---|---|
| `types::FeeRate::from_sat_per_vb(u64) -> Option<FeeRate>` | Fee rate in sat/vB. |
| `types::Balance { confirmed, trusted_pending, untrusted_pending, immature }` | `total()` sums all four; `spendable()` is `confirmed + trusted_pending`. |
| `transaction::Recipient { address, amount }` | One payment output. The address must match the wallet's network. |
| `utxo::Utxo { outpoint, amount, is_change, confirmed }` | A spendable output. |

### Errors (`wallet_lib::error`)

`WalletError`: `InvalidMnemonic`, `Create`, `InvalidAddress`, `InsufficientFunds { needed, available }`, `CreateTx`,
`Signing`, `ApplyUpdate`, `Backend(BackendError)`.
`BackendError`: `Unavailable`, `MalformedData`, `TxNotFound`, `BroadcastRejected`.
All implement `std::error::Error`; `Result<T>` aliases `Result<T, WalletError>`.

## Examples

| Command | What it shows |
|---|---|
| `cargo run --example basic` | The whole API against the mock backend: create, receive, sync, balance, UTXOs, build, sign, broadcast, error handling. |
| `cargo run --example ratatui -- --mock` | Terminal wallet UI, offline with a fake 100,000 sat deposit. |
| `cargo run --example ratatui` | Same UI on signet via Esplora (`mempool.space/signet/api`). |
| `cargo run --example ratatui -- --network testnet --esplora <URL>` | Testnet or a custom Esplora server. |

### TUI keys

- Setup: Up/Down choose generate mnemonic, import mnemonic or import descriptors; Tab next field; Enter create; Esc quit.
- Wallet: Left/Right or `1`-`5` switch tabs (Overview, Receive, UTXOs, Send, Log); `s` sync; `f` sync step by step
  (logs `sync_request`, `fetch`, `apply_update`); `n` new address; `q` quit.
- Send tab: Tab cycles address, amount (sats), fee rate; Enter builds the PSBT; `g` signs; `b` broadcasts; Esc cancels.

## Testing

```sh
cargo test        # integration tests use the mock backend; no network needed
cargo doc --open
```

`ratatui`, `crossterm` and `bdk_esplora` are dev-dependencies, so they are only used by the examples.
