# Wallet Library — Build Guide

## Capstone Goal

Build a reusable Rust crate that provides wallet functionality through a clean public API.

The library is **not an application**. It should be possible for applications such as a Bitcoin CLI, Bitcoin wallet, or another Rust application to depend on the crate without reimplementing wallet logic.

The project will wrap **Bitcoin Dev Kit (BDK)** rather than reimplement Bitcoin wallet primitives from scratch.

The architectural goal is:

```text
Application
    |
    v
Wallet Library API
    |
    v
Your wallet abstractions
    |
    v
BDK
    |
    +----------------------+
    |                      |
    v                      v
ChainBackend           Wallet logic
    |                  /    |    \
    v                 v     v      v
Blockchain          keys  UTXOs  transactions
backend
```

The main learning goals are:

- Rust API design
- traits and abstraction
- dependency injection
- error handling
- testability
- Bitcoin wallet architecture
- BDK architecture
- PSBTs
- descriptors
- UTXOs
- transaction construction
- chain synchronization

---

# 1. MVP Scope

The MVP should expose:

```text
Wallet
├── create from mnemonic
├── create from descriptor
├── new_address()
├── balance()
├── list_utxos()
├── build_tx(recipients, fee_rate)
├── sign(psbt)
├── sync_request()
├── apply_update(update)
└── sync(&backend)        (convenience: composes the two above with a backend)
```

The library should support a pluggable chain backend through a trait.

The backend is **not part of the wallet**. See the design decision below.

---

# 1a. Design Decision — The Backend Is Decoupled From the Wallet

The wallet does not own, store, or take a backend at construction time.

```rust
let wallet = Wallet::from_mnemonic(...)?;   // no backend anywhere
```

`Wallet` is not generic over a backend. `ChainBackend` is a separate capability that the application holds and connects to the wallet when a chain request is needed.

Synchronization is three independent steps:

```text
wallet.sync_request()        Wallet:  "this is what I need to know"
        |
        v
backend.fetch(request)       Backend: "this is what the chain says"
        |
        v
wallet.apply_update(update)  Wallet:  merges the answer into its state
```

Neither side knows the other exists. The caller connects them.

A convenience method composes exactly these steps:

```rust
wallet.sync(&backend)?;
```

Why:

- The wallet works with no backend at all (offline signing, tests that feed a hand-built update).
- The backend can be used and tested without constructing a wallet.
- The backend can change between calls, or one backend can serve several wallets.
- It mirrors BDK's own model: the wallet produces a request, a chain source produces an update, the wallet applies it.

Trade-offs accepted:

- The application holds two objects (wallet and backend).
- `sync` takes the backend as an argument, so the Definition of Done differs from a zero-argument `wallet.sync()`.
- BDK's `SyncRequest` / `Update` types cross the backend boundary. This is confined to the `ChainBackend` trait and must not appear in the wallet's other public methods.

---

# 2. Important Architectural Decision

## We are wrapping BDK

Do not recreate:

- BIP39
- BIP32
- secp256k1
- Bitcoin transaction primitives
- PSBT implementation
- descriptor parsing
- script generation
- signing primitives
- coin-selection algorithms

unless there is a specific educational reason to do so.

BDK already provides wallet functionality and abstractions for these areas.

The capstone is primarily about designing a clean layer **around BDK**.

Study BDK's architecture continuously while implementing the project.

Official BDK resources:

- https://bitcoindevkit.org/
- https://docs.rs/bdk_wallet/
- https://github.com/bitcoindevkit/bdk
- https://github.com/bitcoindevkit/bdk_wallet

---

# 3. Development Strategy

Build from the outside-in conceptually, but implement the blockchain boundary first.

The first milestone is:

```text
ChainBackend
     |
     v
BDK-compatible chain interaction
     |
     v
Mock implementation
```

Do NOT start by implementing `Wallet`.

First understand what the wallet needs from the blockchain.

---

# 4. Phase 1 — Repository Setup

Create the library:

```bash
cargo new wallet-library --lib
cd wallet-library
cargo test
```

Initial structure:

```text
wallet-library/
├── Cargo.toml
├── src/
│   ├── lib.rs
│   ├── backend.rs
│   ├── wallet.rs
│   ├── error.rs
│   ├── types.rs
│   ├── keys.rs
│   ├── address.rs
│   ├── utxo.rs
│   └── transaction.rs
└── tests/
```

Do not implement all modules immediately.

Create modules as responsibilities become clear.

---

# 5. Phase 2 — Understand BDK Before Designing the Wrapper

Before writing `ChainBackend`, study how BDK itself thinks about:

- Wallet
- blockchain data
- transactions
- local wallet state
- syncing
- keychains
- descriptors
- changesets
- persistence
- chain sources

Read the BDK wallet API documentation.

Start with:

- `bdk_wallet::Wallet`
- `bdk_wallet::KeychainKind`
- `bdk_wallet::LocalOutput`
- `bdk_wallet::Balance`
- `bdk_wallet::AddressInfo`
- transaction-related types
- sync-related APIs
- persistence-related APIs

The purpose is not to copy BDK's API.

The purpose is to understand:

> What information does a wallet actually need from a blockchain backend?

---

# 6. Phase 3 — Define the ChainBackend Responsibility

The `ChainBackend` is the boundary between the wallet and the blockchain.

It should handle blockchain interaction.

Conceptually:

```text
              Wallet Library
                   |
                   v
            ChainBackend trait
                   |
        +----------+----------+
        |          |          |
        v          v          v
     Bitcoin    Electrum    Mock
       RPC
```

The wallet should not care whether blockchain data comes from:

- Bitcoin Core
- Electrum
- Esplora
- another API
- a test double

The wallet should depend on the abstraction, and in fact does not depend on it at all: the wallet never holds a backend. It only produces sync requests and consumes updates (see section 1a). The application passes a backend in when it wants to sync or broadcast.

---

# 7. Phase 4 — Determine What ChainBackend Actually Needs

Do not blindly copy methods from another library.

Start by writing down the requirements of each wallet operation.

## `new_address()`

Does not necessarily require a blockchain query.

The wallet can derive an address locally.

Therefore:

```text
new_address()
    |
    +--> Wallet/key management
```

not necessarily:

```text
new_address()
    |
    +--> ChainBackend
```

---

## `balance()`

The wallet needs information about its relevant UTXOs.

Possible flow:

```text
ChainBackend
    |
    v
chain UTXO information
    |
    v
Wallet state
    |
    v
balance()
```

---

## `list_utxos()`

Same principle.

The wallet needs chain information about outputs belonging to the wallet.

---

## `build_tx()`

The wallet needs enough chain state to know which UTXOs can be spent.

But transaction construction itself belongs to wallet/BDK logic.

Therefore:

```text
ChainBackend
    |
    v
UTXO / chain information

Wallet / BDK
    |
    v
coin selection
transaction construction
PSBT creation
```

---

## `sign()`

Signing does not require a chain backend.

The wallet's key material signs the PSBT.

```text
PSBT
 +
private keys
 |
 v
signed PSBT
```

---

## `sync()`

This is the most obvious backend interaction.

```text
Wallet
   |
   | sync_request()
   v
SyncRequest ----> ChainBackend ----> Blockchain
                       |
                       v
                     Update
                       |
                       v
Wallet.apply_update(update)  -->  Wallet state
```

The wallet never calls the backend itself. The application (or the `sync(&backend)` convenience) carries the request to the backend and the update back.

---

# 8. Phase 5 — Design the ChainBackend Trait

Before writing Rust, write the interface in plain English.

Ask:

> What minimum blockchain operations does my wallet abstraction need?

Potential categories include:

### Chain information

- retrieve relevant transaction/output information
- retrieve transaction details
- retrieve confirmation information
- retrieve chain tip information

### Transaction interaction

- broadcast a transaction

### Synchronization

- given a wallet's sync request, return the update needed to change wallet state

The backend never receives or mutates a wallet. It only maps a request to an update.

Expected shape (to be refined after studying BDK):

```text
fetch(request)     -> Update
broadcast(&tx)     -> ()
```

Do not put `balance`, `list_unspent`, `build_tx` or `sign` on the trait. They are wallet operations that work on local state and need no chain access once the wallet is synced.

The exact interface should be decided after studying BDK's current wallet/chain APIs.

Do not make the trait unnecessarily large.

---

# 9. Phase 6 — Design Your Own Domain Types

Avoid leaking every BDK type directly through your public API.

For example, instead of making your entire public API depend on internal BDK structures, consider defining your own:

```text
Balance
Utxo
Recipient
FeeRate
WalletError
```

and converting between your types and BDK types internally.

The goal is:

```text
Application
    |
    v
Your API
    |
    v
BDK adapter
    |
    v
BDK
```

rather than:

```text
Application
    |
    v
Your API
    |
    v
BDK internals everywhere
```

This gives your library room to evolve.

---

# 10. Phase 7 — Build a Mock ChainBackend First

Before connecting to Bitcoin Core, implement:

```text
MockChainBackend
```

The mock should allow tests to provide deterministic blockchain data.

For example:

```text
UTXO A = 100,000 sats
UTXO B = 50,000 sats
UTXO C = 25,000 sats
```

Then your wallet can be tested without:

- a Bitcoin node
- networking
- an Electrum server
- an Esplora API
- testnet infrastructure

This is a major requirement of the architecture.

---

# 11. Phase 8 — Test the Backend Boundary

Write tests proving that your wallet can work against the mock backend.

Examples:

```text
sync retrieves wallet-relevant chain data
sync updates local state
balance reflects synchronized UTXOs
list_utxos returns expected outputs
backend errors propagate correctly
```

Also test failure cases:

```text
backend unavailable
malformed chain data
missing transaction
broadcast failure
```

---

# 12. Phase 9 — Implement the BDK Adapter

Once your `ChainBackend` abstraction is stable, create the implementation that connects it to BDK.

Conceptually:

```text
Your ChainBackend
       |
       v
BDK adapter
       |
       v
BDK chain APIs
```

This adapter is responsible for translating:

```text
Your types
    ↕
BDK types
```

Keep this translation isolated.

---

# 13. Phase 10 — Implement Wallet Creation

After the backend boundary works, implement wallet creation.

Support:

```text
from_mnemonic(...)
```

and:

```text
from_descriptor(...)
```

For mnemonic wallets, understand the complete path:

```text
Mnemonic
   |
   v
BIP39 seed
   |
   v
HD keys
   |
   v
derivation path
   |
   v
descriptor
   |
   v
BDK Wallet
```

Official BIP39:

https://bips.dev/39/

Official BIP32:

https://bips.dev/32/

---

# 14. Phase 11 — Implement `new_address()`

Use BDK's wallet/keychain facilities rather than implementing derivation yourself.

Expected conceptual flow:

```text
Wallet
   |
   v
BDK wallet
   |
   v
external keychain
   |
   v
next unused index
   |
   v
Bitcoin address
```

Test:

```text
first address is deterministic
subsequent addresses differ
address uses correct network
address belongs to expected descriptor
```

---

# 15. Phase 12 — Implement `sync()`

This is where your ChainBackend becomes important.

Conceptual flow:

```text
wallet.sync_request()
     |
     v
SyncRequest
     |
     v
backend.fetch(request)  --> chain source --> chain data
     |
     v
Update
     |
     v
wallet.apply_update(update)
     |
     v
BDK wallet state
```

Implement the pieces in this order:

1. `Wallet::sync_request()`, wrapping BDK's request API.
2. `Wallet::apply_update(update)`, wrapping BDK's `apply_update`.
3. `ChainBackend::fetch(request)`, implemented first by the mock.
4. `Wallet::sync(&backend)`, a thin convenience over 1 to 3.

Test 1 and 2 with hand-built updates and no backend at all.

Study how BDK handles synchronization and changesets before deciding how your wrapper exposes sync.

---

# 16. Phase 13 — Implement `balance()`

Once synchronization works:

```text
Wallet
  |
  v
BDK wallet state
  |
  v
UTXO set
  |
  +--> confirmed
  |
  +--> unconfirmed
```

Expose a clean `Balance` type from your library.

Test:

```text
zero balance
confirmed balance
unconfirmed balance
combined balance
```

---

# 17. Phase 14 — Implement `list_utxos()`

Expose wallet-owned spendable outputs.

Conceptually:

```text
Wallet
   |
   v
BDK wallet
   |
   v
local outputs
   |
   v
Vec<Utxo>
```

Your own `Utxo` type should contain only the information your public API actually promises.

---

# 18. Phase 15 — Implement `build_tx()`

Required API:

```text
build_tx(recipients, fee_rate)
```

Conceptual flow:

```text
Recipients
     |
     v
validate recipients
     |
     v
BDK transaction builder
     |
     +--> select UTXOs
     |
     +--> calculate fee
     |
     +--> calculate change
     |
     v
PSBT
```

Do not implement coin selection from scratch unless the project explicitly requires it.

Use BDK's transaction builder and coin-selection facilities.

Your job is to expose a clean abstraction around them.

---

# 19. Phase 16 — Understand PSBT

Before implementing signing, study PSBT.

Official specification:

https://bips.dev/174/

Mental model:

```text
PSBT
├── unsigned transaction
├── input information
├── UTXO information
├── derivation information
└── signatures / signing data
```

The important distinction:

```text
PSBT != signed transaction
```

---

# 20. Phase 17 — Implement `sign()`

Your API:

```text
sign(psbt)
```

Conceptually:

```text
PSBT
  |
  v
BDK wallet
  |
  v
identify wallet-owned inputs
  |
  v
sign
  |
  v
signed PSBT
```

Do not expose private keys through the public API.

The wallet should retain control of its signing material.

---

# 21. Phase 18 — Add Descriptor Support

Descriptors are important because they describe how wallet outputs are constructed.

Study:

https://bips.dev/380/

Understand:

- descriptor syntax
- checksums
- key expressions
- derivation paths
- external/change keychains

Then define exactly which descriptor forms your MVP supports.

Document the supported subset.

---

# 22. Phase 19 — Add a Real Chain Backend

Only after the mock implementation works should you connect to a real blockchain source.

Possible backends:

```text
Bitcoin Core
Electrum
Esplora
```

Do not implement all three initially.

Choose one.

Then:

```text
Application
   |
   +--> Wallet          (no backend inside)
   |
   +--> ConcreteBackend (implements ChainBackend)
            |
            v
   real Bitcoin infrastructure
```

The important test is:

> Can I replace the concrete backend without changing wallet logic?

If yes, the abstraction is doing its job.

---

# 23. Testing Strategy

Use three levels of testing.

## Unit tests

Test individual components:

```text
address derivation
error conversion
UTXO conversion
balance calculation
recipient validation
```

## Backend tests

Test:

```text
MockChainBackend
BDK adapter
backend failures
```

## Integration tests

Test complete workflows:

```text
create wallet
    ↓
derive address
    ↓
mock chain data
    ↓
sync
    ↓
check balance
    ↓
list UTXOs
    ↓
build PSBT
    ↓
sign PSBT
```

---

# 24. Recommended Project Structure

As the project matures, aim for something approximately like:

```text
src/
├── lib.rs
│
├── wallet/
│   ├── mod.rs
│   ├── wallet.rs
│   └── builder.rs
│
├── backend/
│   ├── mod.rs
│   ├── trait.rs
│   ├── bdk.rs
│   └── mock.rs
│
├── transaction/
│   ├── mod.rs
│   ├── recipient.rs
│   ├── builder.rs
│   └── psbt.rs
│
├── keys/
│   ├── mod.rs
│   ├── mnemonic.rs
│   └── descriptor.rs
│
├── types/
│   ├── mod.rs
│   ├── balance.rs
│   ├── utxo.rs
│   └── fee_rate.rs
│
└── error.rs
```

Do not force this structure from day one.

Refactor toward it as responsibilities become clear.

---

# 25. Definition of Done

The MVP is complete when an external Rust application can conceptually do:

```rust
let wallet = Wallet::from_mnemonic(...)?;

let address = wallet.new_address()?;

let backend = /* any ChainBackend implementation */;
wallet.sync(&backend)?;

let balance = wallet.balance()?;

let utxos = wallet.list_utxos()?;

let psbt = wallet.build_tx(
    recipients,
    fee_rate,
)?;

let signed_psbt = wallet.sign(psbt)?;
```

without needing to know the internal BDK implementation.

---

# 26. Architectural Success Criteria

Your implementation should satisfy these questions.

### Backend abstraction

- Can the wallet operate with a mock backend?
- Can the real backend be replaced?
- Does wallet logic avoid HTTP/RPC/Electrum-specific code?
- Can a wallet be created and used (address, sign) with no backend in existence?
- Does the backend work without ever seeing a wallet?
- Is `Wallet` free of a backend type parameter?

### BDK wrapper

- Is BDK implementation detail isolated?
- Are unnecessary BDK types exposed publicly?
- Does your API represent your own domain?

### Wallet

- Does wallet state remain separate from chain access?
- Is key management separate from synchronization?
- Is transaction construction separate from broadcasting?

### Testing

- Can most tests run without a Bitcoin node?
- Are backend failures testable?
- Are deterministic Bitcoin test vectors used?

### Security

- Are private keys kept inside appropriate wallet structures?
- Are secrets excluded from logs?
- Are network mismatches rejected?
- Are transaction amounts checked carefully?

---

# 27. Learning Materials

## Rust

Official Rust Book:

https://doc.rust-lang.org/book/

Rust API Guidelines:

https://rust-lang.github.io/api-guidelines/

Rust traits:

https://doc.rust-lang.org/book/ch10-02-traits.html

Rust error handling:

https://doc.rust-lang.org/book/ch09-00-error-handling.html

---

## Bitcoin

Bitcoin Developer Documentation:

https://developer.bitcoin.org/

Bitcoin BIPs:

https://bips.dev/

BIP32:

https://bips.dev/32/

BIP39:

https://bips.dev/39/

BIP174 / PSBT:

https://bips.dev/174/

BIP380 / Descriptors:

https://bips.dev/380/

---

## BDK

BDK official site:

https://bitcoindevkit.org/

BDK wallet documentation:

https://docs.rs/bdk_wallet/

BDK GitHub:

https://github.com/bitcoindevkit/bdk

BDK wallet repository:

https://github.com/bitcoindevkit/bdk_wallet

---

## rust-bitcoin

Documentation:

https://docs.rs/bitcoin/

Repository:

https://github.com/rust-bitcoin/rust-bitcoin

---

# 28. First Milestone — START HERE

Do not implement the entire wallet yet.

Your immediate objective is:

```text
MILESTONE 1

Understand BDK's chain architecture
             ↓
Identify what your wallet actually needs
             ↓
Design ChainBackend trait
             ↓
Define your domain types
             ↓
Implement MockChainBackend
             ↓
Write tests
```

Before moving on, you should be able to explain:

1. What information comes from the blockchain?
2. What information belongs to the wallet?
3. What does BDK already provide?
4. What should your library expose?
5. What should your library hide?
6. Why is `ChainBackend` a trait?
7. How would a mock backend allow testing without Bitcoin?
8. How would a real backend replace the mock?
9. Why does the wallet not hold a backend, and how do a request and an update travel between them?

Only after those questions are clear should you implement the BDK adapter.

---

# 29. Working Rule for the Capstone

For every abstraction you introduce, ask:

```text
1. What problem does this abstraction solve?

2. Who owns this responsibility?

3. What does the caller actually need to know?

4. Can I test this without a real blockchain?

5. Am I duplicating functionality BDK already provides?

6. Am I leaking BDK implementation details into my public API?
```

The objective is not to write the most code.

The objective is to create a small, coherent wallet library whose architecture makes sense.
