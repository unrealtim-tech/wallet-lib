use std::cell::RefCell;
use std::str::FromStr;
use std::sync::Arc;
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::thread;
use std::time::{Duration, Instant};

use bdk_wallet::Update;
use bdk_wallet::keys::bip39::{Language, Mnemonic, WordCount};
use bdk_wallet::keys::{GeneratableKey, GeneratedKey};
use bdk_wallet::miniscript::Segwitv0;
use bitcoin::hashes::Hash;
use bitcoin::{
    Address, Amount, Network, OutPoint, Psbt, Transaction, TxIn, TxOut, Txid, absolute, transaction,
};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::{Position, Rect};
use wallet_lib::backend::mock::MockChainBackend;
use wallet_lib::error::{BackendError, WalletError};
use wallet_lib::transaction::Recipient;
use wallet_lib::types::FeeRate;
use wallet_lib::{ChainBackend, Wallet};

use crate::actions::Action;
use crate::backend::EsploraBackend;
use crate::clipboard;
use crate::friendly::{bad, explain, info, ok};
use crate::rpc_backend::Progress;
use crate::rpc_backend::RpcBackend;

pub const TABS: [&str; 5] = ["Overview", "Receive", "UTXOs", "Send", "Log"];

/// How often the wallet re-syncs on its own.
pub const AUTO_SYNC_EVERY: Duration = Duration::from_secs(15);

/// `Send` below is this module's payment struct, so the thread-safety bound is spelled out here.
pub type SharedBackend = Arc<dyn ChainBackend + std::marker::Send + Sync>;

/// A sync whose network fetch is running on a worker thread.
pub struct PendingSync {
    rx: Receiver<Result<Update, BackendError>>,
    stepwise: bool,
    auto: bool,
}

/// Which chain source the session uses.
pub enum BackendChoice {
    Mock,
    Esplora(String),
    /// Bitcoin Core RPC: url, user, password.
    Rpc(String, String, String),
}

#[derive(Default)]
pub struct Input {
    pub text: String,
}

impl Input {
    fn edit(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Char(c) if !key.modifiers.contains(KeyModifiers::CONTROL) => self.text.push(c),
            KeyCode::Backspace => {
                self.text.pop();
            }
            _ => {}
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum SetupMode {
    NewMnemonic,
    ImportMnemonic,
    Descriptor,
}

impl SetupMode {
    pub const ALL: [(SetupMode, &'static str); 3] = [
        (SetupMode::NewMnemonic, "Generate a new mnemonic"),
        (SetupMode::ImportMnemonic, "Import an existing mnemonic"),
        (
            SetupMode::Descriptor,
            "Import descriptors (xprv = can sign, xpub = watch-only)",
        ),
    ];
}

pub struct Setup {
    pub mode: usize,
    pub field: usize,
    pub mnemonic: Input,
    pub passphrase: Input,
    pub external: Input,
    pub internal: Input,
    pub generated: Option<String>,
}

impl Setup {
    fn new() -> Self {
        Self {
            mode: 0,
            field: 0,
            mnemonic: Input::default(),
            passphrase: Input::default(),
            external: Input::default(),
            internal: Input::default(),
            generated: None,
        }
    }

    pub fn mode(&self) -> SetupMode {
        SetupMode::ALL[self.mode].0
    }

    /// Text inputs relevant to the current mode, with labels.
    pub fn fields(&self) -> Vec<(&'static str, &Input)> {
        match self.mode() {
            SetupMode::NewMnemonic => vec![("BIP39 passphrase (optional)", &self.passphrase)],
            SetupMode::ImportMnemonic => {
                vec![
                    ("Mnemonic", &self.mnemonic),
                    ("BIP39 passphrase (optional)", &self.passphrase),
                ]
            }
            SetupMode::Descriptor => vec![
                ("External (receive) descriptor", &self.external),
                ("Internal (change) descriptor, optional", &self.internal),
            ],
        }
    }

    fn active_input(&mut self) -> &mut Input {
        let mode = self.mode();
        let field = self.field;
        match (mode, field) {
            (SetupMode::NewMnemonic, _) => &mut self.passphrase,
            (SetupMode::ImportMnemonic, 0) => &mut self.mnemonic,
            (SetupMode::ImportMnemonic, _) => &mut self.passphrase,
            (SetupMode::Descriptor, 0) => &mut self.external,
            (SetupMode::Descriptor, _) => &mut self.internal,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SendField {
    Address,
    Amount,
    FeeRate,
}

/// Progress of a payment through build -> sign -> broadcast.
pub enum Stage {
    Editing,
    Built(Psbt),
    Signed(Transaction),
    Broadcast(bitcoin::Txid),
}

pub struct Send {
    pub address: Input,
    pub amount: Input,
    pub fee_rate: Input,
    pub field: SendField,
    pub stage: Stage,
    /// Network fee of the PSBT built last.
    pub fee: Option<Amount>,
}

impl Send {
    fn new() -> Self {
        Self {
            address: Input::default(),
            amount: Input::default(),
            fee_rate: Input { text: "2".into() },
            field: SendField::Address,
            stage: Stage::Editing,
            fee: None,
        }
    }

    fn active_input(&mut self) -> &mut Input {
        match self.field {
            SendField::Address => &mut self.address,
            SendField::Amount => &mut self.amount,
            SendField::FeeRate => &mut self.fee_rate,
        }
    }
}

pub struct Session {
    pub wallet: Wallet,
    pub backend: SharedBackend,
    pub backend_label: String,
    pub tab: usize,
    pub address: Option<Address>,
    pub send: Send,
    pub pending: Option<PendingSync>,
    pub last_sync: Instant,
    /// Present only for a regtest wallet on the bitcoind RPC backend.
    pub miner: Option<Arc<RpcBackend>>,
    pub utxo_sel: usize,
    /// Block-scan progress (RPC backend only).
    pub progress: Option<Arc<Progress>>,
    /// Outcome of the latest sync; unlike the status bar it is not overwritten by other messages.
    pub sync_status: String,
    pub sync_ok: bool,
    pub synced_at: Option<Instant>,
}

impl Session {
    /// Time until the next automatic sync, or `None` while a sync is running.
    pub fn next_sync_in(&self) -> Option<Duration> {
        self.pending
            .is_none()
            .then(|| AUTO_SYNC_EVERY.saturating_sub(self.last_sync.elapsed()))
    }
}

/// Shown once, right after a new mnemonic is generated.
pub struct Backup {
    pub words: String,
    pub session: Box<Session>,
}

pub enum Screen {
    Setup(Setup),
    Backup(Box<Backup>),
    Wallet(Box<Session>),
}

pub struct App {
    pub network: Network,
    pub choice: BackendChoice,
    pub screen: Screen,
    pub log: Vec<String>,
    pub status: String,
    pub should_quit: bool,
    /// Clickable regions from the last draw.
    pub hits: RefCell<Vec<(Rect, Action)>>,
    /// Log lines scrolled up from the bottom.
    pub log_scroll: usize,
}

impl App {
    pub fn new(network: Network, choice: BackendChoice) -> Self {
        Self {
            network,
            choice,
            screen: Screen::Setup(Setup::new()),
            log: Vec::new(),
            status: "Choose how to create the wallet. Up/Down: mode, Tab: field, Enter: create, Esc: quit".into(),
            should_quit: false,
            hits: RefCell::new(Vec::new()),
            log_scroll: 0,
        }
    }

    fn note(&mut self, msg: impl Into<String>) {
        let msg = msg.into();
        self.status = msg.clone();
        self.log.push(msg);
    }

    pub fn on_key(&mut self, key: KeyEvent) {
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
            self.should_quit = true;
            return;
        }
        match &self.screen {
            Screen::Setup(_) => self.on_key_setup(key),
            Screen::Backup(_) => match key.code {
                KeyCode::Char('c') => self.dispatch(Action::CopyMnemonic),
                KeyCode::Enter => self.dispatch(Action::ConfirmBackup),
                _ => {}
            },
            Screen::Wallet(_) => self.on_key_wallet(key),
        }
    }

    pub fn on_mouse(&mut self, m: MouseEvent) {
        match m.kind {
            MouseEventKind::Down(MouseButton::Left) => {
                let at = Position::new(m.column, m.row);
                let hit = self.hits.borrow().iter().rev().find(|(r, _)| r.contains(at)).map(|(_, a)| *a);
                if let Some(action) = hit {
                    self.dispatch(action);
                }
            }
            MouseEventKind::ScrollUp => self.scroll(-3),
            MouseEventKind::ScrollDown => self.scroll(3),
            _ => {}
        }
    }

    /// Moves the UTXO selection or the log view, whichever is on screen.
    fn scroll(&mut self, delta: isize) {
        let Screen::Wallet(s) = &mut self.screen else {
            return;
        };
        match s.tab {
            2 => {
                let last = s.wallet.list_utxos().len().saturating_sub(1);
                s.utxo_sel = s.utxo_sel.saturating_add_signed(delta.signum()).min(last);
            }
            4 => {
                let max = self.log.len().saturating_sub(1);
                self.log_scroll = self.log_scroll.saturating_add_signed(-delta).min(max);
            }
            _ => {}
        }
    }

    /// Single entry point for keys and clicks.
    pub fn dispatch(&mut self, action: Action) {
        match action {
            Action::SetMode(i) => {
                if let Screen::Setup(setup) = &mut self.screen {
                    setup.mode = i;
                    setup.field = 0;
                }
            }
            Action::SetupField(i) => {
                if let Screen::Setup(setup) = &mut self.screen {
                    setup.field = i;
                }
            }
            Action::Create => self.create_wallet(),
            Action::CopyMnemonic => {
                if let Screen::Backup(b) = &self.screen {
                    let via = clipboard::copy(&b.words);
                    self.status = ok(format!("Recovery phrase copied to {via}. Store it somewhere safe, then clear your clipboard."));
                }
            }
            Action::ConfirmBackup => {
                if let Screen::Backup(b) = std::mem::replace(&mut self.screen, Screen::Setup(Setup::new())) {
                    let network = b.session.wallet.network();
                    self.screen = Screen::Wallet(b.session);
                    self.note(ok(format!("Your wallet is ready on {network}. Press Sync to load your balance.")));
                }
            }
            Action::Tab(i) => {
                if let Screen::Wallet(s) = &mut self.screen {
                    s.tab = i;
                }
            }
            Action::Sync => self.start_sync(false, false),
            Action::SyncStep => self.start_sync(true, false),
            Action::NewAddress => self.new_address(),
            Action::Mine(n) => self.mine(n),
            Action::CopyAddress => {
                let Screen::Wallet(s) = &self.screen else { return };
                if let Some(a) = s.address.as_ref().map(|a| a.to_string()) {
                    let via = clipboard::copy(&a);
                    self.note(ok(format!("Address copied to {via}: {a}")));
                }
            }
            Action::CopyOutpoint => {
                let Screen::Wallet(s) = &self.screen else { return };
                match s.wallet.list_utxos().get(s.utxo_sel).map(|u| u.outpoint.to_string()) {
                    Some(o) => {
                        let via = clipboard::copy(&o);
                        self.note(ok(format!("Coin ID copied to {via}.")));
                    }
                    None => self.note(bad("There are no coins to select yet. Receive some first, then press Sync.")),
                }
            }
            Action::CopyLog => {
                let via = clipboard::copy(&self.log.join("\n"));
                self.note(ok(format!("Activity log ({} entries) copied to {via}.", self.log.len())));
            }
            Action::SelectUtxo(i) => {
                if let Screen::Wallet(s) = &mut self.screen {
                    s.utxo_sel = i;
                }
            }
            Action::SendField(f) => {
                if let Screen::Wallet(s) = &mut self.screen {
                    if matches!(s.send.stage, Stage::Editing) {
                        s.send.field = f;
                    }
                }
            }
            Action::Build => self.build_tx(),
            Action::Sign => self.sign(),
            Action::Broadcast => self.broadcast(),
            Action::CancelPayment => {
                if let Screen::Wallet(s) = &mut self.screen {
                    s.send.stage = Stage::Editing;
                }
                self.note(info("Payment cancelled. Nothing was sent."));
            }
            Action::NewPayment => {
                if let Screen::Wallet(s) = &mut self.screen {
                    s.send = Send::new();
                }
            }
            Action::Quit => self.should_quit = true,
        }
    }

    /// Regtest only: mines `n` blocks to the wallet's current receive address, then syncs.
    fn mine(&mut self, n: u64) {
        let Screen::Wallet(s) = &mut self.screen else {
            return;
        };
        let Some(miner) = s.miner.clone() else {
            return self.note(bad("Mining only works on a local test network (regtest) connected to your Bitcoin node."));
        };
        let address = match &s.address {
            Some(a) => a.clone(),
            None => s.wallet.new_address(),
        };
        match miner.mine(n, &address) {
            Ok(count) => {
                self.log.push(info(format!("Mined {count} new block(s). The mining reward was sent to your address {address}.")));
                self.start_sync(false, false);
                if n < 101 {
                    self.status = ok(format!("Mined {count} block(s). Mining rewards stay locked until 100 more blocks are mined."));
                } else {
                    self.status = ok(format!("Mined {count} blocks. Your first mining reward is now unlocked and ready to spend."));
                }
            }
            Err(e) => self.note(bad(format!("Mining did not work. {}", explain(&e.to_string())))),
        }
    }

    // ---------- setup ----------

    fn on_key_setup(&mut self, key: KeyEvent) {
        let Screen::Setup(setup) = &mut self.screen else {
            return;
        };
        match key.code {
            KeyCode::Esc => self.should_quit = true,
            KeyCode::Up => {
                setup.mode = (setup.mode + SetupMode::ALL.len() - 1) % SetupMode::ALL.len();
                setup.field = 0;
            }
            KeyCode::Down => {
                setup.mode = (setup.mode + 1) % SetupMode::ALL.len();
                setup.field = 0;
            }
            KeyCode::Tab => {
                let n = setup.fields().len();
                setup.field = (setup.field + 1) % n;
            }
            KeyCode::Enter => self.create_wallet(),
            _ => setup.active_input().edit(key),
        }
    }

    fn create_wallet(&mut self) {
        let network = self.network;
        let Screen::Setup(setup) = &mut self.screen else {
            return;
        };
        let created = match setup.mode() {
            SetupMode::NewMnemonic => {
                let words = match &setup.generated {
                    Some(w) => w.clone(),
                    None => {
                        let m: GeneratedKey<Mnemonic, Segwitv0> =
                            Mnemonic::generate((WordCount::Words12, Language::English))
                                .expect("entropy available");
                        m.into_key().to_string()
                    }
                };
                setup.generated = Some(words.clone());
                Wallet::from_mnemonic(&words, &setup.passphrase.text, network)
            }
            SetupMode::ImportMnemonic => {
                Wallet::from_mnemonic(setup.mnemonic.text.trim(), &setup.passphrase.text, network)
            }
            SetupMode::Descriptor => {
                let internal = setup.internal.text.trim();
                Wallet::from_descriptor(
                    setup.external.text.trim(),
                    (!internal.is_empty()).then_some(internal),
                    network,
                )
            }
        };
        let generated = setup.generated.clone();
        let mode = setup.mode();
        match created {
            Ok(wallet) => self.start_session(wallet, mode, generated),
            Err(e) => self.note(bad(format!("Could not create the wallet. {}", explain(&e.to_string())))),
        }
    }

    fn start_session(&mut self, mut wallet: Wallet, mode: SetupMode, generated: Option<String>) {
        let address = wallet.new_address();
        let mut miner = None;
        let mut progress = None;
        let (backend, label): (SharedBackend, String) = match &self.choice {
            BackendChoice::Mock => (
                Arc::new(MockChainBackend::with_update(fake_deposit(
                    &address, 100_000,
                ))),
                "mock (in-memory, offline)".into(),
            ),
            BackendChoice::Esplora(url) => match EsploraBackend::new(url) {
                Ok(b) => {
                    let label = format!("esplora {}", b.url());
                    (Arc::new(b), label)
                }
                Err(e) => {
                    self.note(bad(format!("Could not connect to the chain source. {}", explain(&e.to_string()))));
                    return;
                }
            },
            BackendChoice::Rpc(url, user, pass) => match RpcBackend::new(url, user, pass) {
                Ok(b) => {
                    let label = format!("bitcoind rpc {}", b.url());
                    progress = Some(b.progress());
                    let b = Arc::new(b);
                    if self.network == Network::Regtest {
                        miner = Some(Arc::clone(&b));
                    }
                    (b, label)
                }
                Err(e) => {
                    self.note(bad(format!("Could not connect to the chain source. {}", explain(&e.to_string()))));
                    return;
                }
            },
        };
        let session = Box::new(Session {
            wallet,
            backend,
            backend_label: label.clone(),
            tab: 0,
            address: Some(address),
            send: Send::new(),
            pending: None,
            last_sync: Instant::now(),
            miner,
            utxo_sel: 0,
            progress,
            sync_status: "Not refreshed yet. Press Sync.".into(),
            sync_ok: true,
            synced_at: None,
        });
        if let (SetupMode::NewMnemonic, Some(words)) = (mode, generated) {
            // The phrase is shown once on its own screen and never written to the log.
            self.status = info("Write down your recovery phrase now. It is shown only once.");
            self.screen = Screen::Backup(Box::new(Backup { words, session }));
            return;
        }
        self.note(ok(format!(
            "Your wallet is ready on {} (connected to {label}). Press Sync to load your balance.",
            session.wallet.network()
        )));
        self.screen = Screen::Wallet(session);
    }

    // ---------- wallet ----------

    fn on_key_wallet(&mut self, key: KeyEvent) {
        let editing = matches!(&self.screen, Screen::Wallet(s) if s.tab == 3 && matches!(s.send.stage, Stage::Editing));
        let Screen::Wallet(s) = &mut self.screen else {
            return;
        };
        match key.code {
            KeyCode::Esc if !editing && s.tab != 3 => self.should_quit = true,
            KeyCode::Right => s.tab = (s.tab + 1) % TABS.len(),
            KeyCode::Left => s.tab = (s.tab + TABS.len() - 1) % TABS.len(),
            KeyCode::Char(c) if !editing => match c {
                'q' => self.should_quit = true,
                's' => self.start_sync(false, false),
                'f' => self.start_sync(true, false),
                'n' => self.new_address(),
                '1'..='5' => s.tab = c as usize - '1' as usize,
                'g' => self.sign(),
                'b' => self.broadcast(),
                'm' => self.mine(1),
                'M' => self.mine(101),
                'c' => match s.tab {
                    2 => self.dispatch(Action::CopyOutpoint),
                    4 => self.dispatch(Action::CopyLog),
                    _ => self.dispatch(Action::CopyAddress),
                },
                _ => {}
            },
            KeyCode::Up if !editing => self.scroll(-1),
            KeyCode::Down if !editing => self.scroll(1),
            KeyCode::PageUp if !editing => self.scroll(-10),
            KeyCode::PageDown if !editing => self.scroll(10),
            KeyCode::Home if !editing && s.tab == 4 => self.log_scroll = self.log.len().saturating_sub(1),
            KeyCode::End if !editing && s.tab == 4 => self.log_scroll = 0,
            _ if s.tab == 3 => self.on_key_send(key),
            _ => {}
        }
    }

    fn on_key_send(&mut self, key: KeyEvent) {
        let Screen::Wallet(s) = &mut self.screen else {
            return;
        };
        match (&s.send.stage, key.code) {
            (Stage::Editing, KeyCode::Tab) => {
                s.send.field = match s.send.field {
                    SendField::Address => SendField::Amount,
                    SendField::Amount => SendField::FeeRate,
                    SendField::FeeRate => SendField::Address,
                };
            }
            (Stage::Editing, KeyCode::Enter) => self.build_tx(),
            (Stage::Editing, KeyCode::Esc) => self.should_quit = true,
            (Stage::Editing, _) => s.send.active_input().edit(key),
            (_, KeyCode::Esc) => {
                s.send.stage = Stage::Editing;
                self.note("Payment cancelled.");
            }
            (Stage::Broadcast(_), KeyCode::Enter) => s.send = Send::new(),
            _ => {}
        }
    }

    /// Builds the request here, runs `backend.fetch` on a worker thread, and leaves the result
    /// in `pending` for [`App::tick`] to apply. `stepwise` logs the three API steps separately.
    fn start_sync(&mut self, stepwise: bool, auto: bool) {
        let Screen::Wallet(s) = &mut self.screen else {
            return;
        };
        if s.pending.is_some() {
            return self.note(info("A refresh is already in progress."));
        }
        let request = s.wallet.sync_request();
        let spks = request.progress().spks_remaining;
        let backend = Arc::clone(&s.backend);
        let (tx, rx) = mpsc::channel();
        thread::spawn(move || {
            let _ = tx.send(backend.fetch(request));
        });
        s.pending = Some(PendingSync { rx, stepwise, auto });
        // Reset at start, so a failing backend is retried after the interval, not every tick.
        s.last_sync = Instant::now();
        if stepwise {
            self.log.push(info(format!(
                "Step 1 of 3: preparing {spks} address(es) to look up"
            )));
        }
        if auto {
            self.log.push(info("Refreshing your balance automatically..."));
        } else {
            self.note(info("Checking the blockchain for your transactions..."));
        }
    }

    /// Called every loop iteration: applies a finished sync, or starts an automatic one when due.
    pub fn tick(&mut self) {
        let Screen::Wallet(s) = &mut self.screen else {
            return;
        };
        let fetched = match s.pending.as_ref().map(|p| p.rx.try_recv()) {
            None => {
                if s.last_sync.elapsed() >= AUTO_SYNC_EVERY {
                    self.start_sync(false, true);
                }
                return;
            }
            Some(Err(TryRecvError::Empty)) => return,
            Some(Ok(result)) => result,
            Some(Err(TryRecvError::Disconnected)) => {
                Err(BackendError::Unavailable("sync thread stopped".into()))
            }
        };
        let pending = s.pending.take().expect("checked above");
        let what = if pending.auto { "Automatic refresh" } else { "Refresh" };
        let result = match fetched {
            Ok(update) => {
                if pending.stepwise {
                    self.log.push(info("Step 2 of 3: received the latest blockchain data"));
                }
                s.wallet.apply_update(update).inspect(|_| {
                    if pending.stepwise {
                        self.log.push(info("Step 3 of 3: updated your wallet"));
                    }
                })
            }
            Err(e) => Err(WalletError::from(e)),
        };
        match result {
            Ok(()) => {
                let b = s.wallet.balance();
                let msg = ok(format!(
                    "{what} complete. You have {} in total, {} ready to spend.",
                    b.total(),
                    b.spendable()
                ));
                s.sync_ok = true;
                s.sync_status = format!("Up to date: {} total, {} ready to spend", b.total(), b.spendable());
                s.synced_at = Some(Instant::now());
                // Automatic syncs stay out of the status bar so they do not hide other messages.
                if pending.auto {
                    self.log.push(msg);
                } else {
                    self.note(msg);
                }
            }
            Err(e) => {
                s.sync_ok = false;
                s.sync_status = format!("Could not refresh. {}", explain(&e.to_string()));
                s.synced_at = Some(Instant::now());
                self.note(bad(format!("{what} failed. {}", explain(&e.to_string()))));
            }
        }
    }

    fn new_address(&mut self) {
        let Screen::Wallet(s) = &mut self.screen else {
            return;
        };
        let address = s.wallet.new_address();
        s.tab = 1;
        let msg = ok(format!("New receive address ready: {address}"));
        s.address = Some(address);
        self.note(msg);
    }

    fn build_tx(&mut self) {
        let network = self.network;
        let Screen::Wallet(s) = &mut self.screen else {
            return;
        };
        let parsed = (|| -> Result<(Recipient, FeeRate), String> {
            let address = Address::from_str(s.send.address.text.trim())
                .map_err(|e| format!("address: {e}"))?
                .require_network(network)
                .map_err(|e| format!("address: {e}"))?;
            let sats: u64 = s
                .send
                .amount
                .text
                .trim()
                .parse()
                .map_err(|_| "amount must be whole sats")?;
            let rate: u64 = s
                .send
                .fee_rate
                .text
                .trim()
                .parse()
                .map_err(|_| "fee rate must be whole sat/vB")?;
            let fee_rate = FeeRate::from_sat_per_vb(rate).ok_or("fee rate too large")?;
            Ok((
                Recipient {
                    address,
                    amount: Amount::from_sat(sats),
                },
                fee_rate,
            ))
        })();
        let (recipient, fee_rate) = match parsed {
            Ok(v) => v,
            Err(e) => return self.note(bad(format!("Please check the form: {e}"))),
        };
        match s.wallet.build_tx(&[recipient], fee_rate) {
            Ok(psbt) => {
                s.send.fee = psbt.fee().ok();
                s.send.stage = Stage::Built(psbt);
                self.note(ok("Payment prepared. Review the details, then press Sign (g), or Esc to cancel."));
            }
            Err(e) => self.note(bad(format!("Could not prepare the payment. {}", explain(&e.to_string())))),
        }
    }

    fn sign(&mut self) {
        let Screen::Wallet(s) = &mut self.screen else {
            return;
        };
        let Stage::Built(psbt) = &s.send.stage else {
            return;
        };
        match s.wallet.sign(psbt.clone()) {
            Ok(tx) => {
                let msg = ok("Payment signed with your keys. Press Broadcast (b) to send it.");
                s.send.stage = Stage::Signed(tx);
                self.note(msg);
            }
            Err(e) => self.note(bad(format!("Could not sign the payment. {}", explain(&e.to_string())))),
        }
    }

    fn broadcast(&mut self) {
        let Screen::Wallet(s) = &mut self.screen else {
            return;
        };
        let Stage::Signed(tx) = &s.send.stage else {
            return;
        };
        match s.wallet.broadcast(s.backend.as_ref(), tx) {
            Ok(()) => {
                let txid = tx.compute_txid();
                s.send.stage = Stage::Broadcast(txid);
                self.note(ok(format!(
                    "Payment sent to the network (ID {txid}). It is confirmed once it is included in a block; press Sync to check."
                )));
            }
            Err(e) => self.note(bad(format!("The payment was not sent. {}", explain(&e.to_string())))),
        }
    }
}

/// Fabricates a deposit so the mock backend has something to sync.
fn fake_deposit(address: &Address, sats: u64) -> Update {
    let tx = Transaction {
        version: transaction::Version::TWO,
        lock_time: absolute::LockTime::ZERO,
        input: vec![TxIn {
            previous_output: OutPoint::new(Txid::from_byte_array([7; 32]), 0),
            ..Default::default()
        }],
        output: vec![TxOut {
            value: Amount::from_sat(sats),
            script_pubkey: address.script_pubkey(),
        }],
    };
    let mut update = Update::default();
    update
        .tx_update
        .seen_ats
        .insert((tx.compute_txid(), 1_700_000_000));
    update.tx_update.txs.push(Arc::new(tx));
    update
}
