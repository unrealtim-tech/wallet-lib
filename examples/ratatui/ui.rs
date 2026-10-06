//! Rendering. Reads wallet state through the public `wallet-lib` API only.
//!
//! Every clickable region is registered in `app.hits` while drawing, so the mouse handler
//! always matches what is on screen.

use ratatui::Frame;
use ratatui::layout::{Alignment, Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style, Stylize};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, Row, Table, Wrap};
use std::str::FromStr;
use std::sync::atomic::Ordering;

use bitcoin::{Address, Amount, ScriptBuf, Transaction};

use crate::actions::Action;
use crate::app::{App, Backup, Input, Screen, SendField, Session, Setup, SetupMode, Stage, TABS};

const ACCENT: Color = Color::Cyan;
const OK: Color = Color::Green;
const WARN: Color = Color::Yellow;
const ERR: Color = Color::Red;

pub fn draw(f: &mut Frame, app: &App) {
    app.hits.borrow_mut().clear();
    let [body, actions, status] =
        Layout::vertical([Constraint::Min(0), Constraint::Length(3), Constraint::Length(3)]).areas(f.area());
    let buttons: Vec<(String, Action, Color)> = match &app.screen {
        Screen::Setup(setup) => {
            draw_setup(f, body, app, setup);
            vec![("Create wallet (Enter)".into(), Action::Create, OK), ("Quit (Esc)".into(), Action::Quit, ERR)]
        }
        Screen::Backup(b) => {
            draw_backup(f, body, b);
            vec![
                ("Copy phrase (c)".into(), Action::CopyMnemonic, ACCENT),
                ("I've saved it - continue (Enter)".into(), Action::ConfirmBackup, OK),
            ]
        }
        Screen::Wallet(s) => {
            let [tabs, main] = Layout::vertical([Constraint::Length(3), Constraint::Min(0)]).areas(body);
            draw_tabs(f, tabs, app, s);
            match s.tab {
                0 => draw_overview(f, main, app, s),
                1 => draw_receive(f, main, app, s),
                2 => draw_utxos(f, main, app, s),
                3 => draw_send(f, main, app, s),
                _ => draw_log(f, main, app),
            }
            wallet_buttons(s)
        }
    };
    button_row(f, actions, app, &buttons);
    let color = if is_error(&app.status) { ERR } else { OK };
    f.render_widget(
        Paragraph::new(app.status.as_str())
            .wrap(Wrap { trim: true })
            .style(Style::new().fg(color))
            .block(Block::bordered().border_style(Style::new().fg(color)).title("Status")),
        status,
    );
}

fn is_error(text: &str) -> bool {
    crate::friendly::is_bad(text)
}

fn hit(app: &App, rect: Rect, action: Action) {
    app.hits.borrow_mut().push((rect, action));
}

/// Draws bordered buttons left to right (dropping any that do not fit) and registers clicks.
fn button_row(f: &mut Frame, area: Rect, app: &App, buttons: &[(String, Action, Color)]) {
    let mut x = area.x;
    for (label, action, color) in buttons {
        let w = label.chars().count() as u16 + 4;
        if x + w > area.x + area.width {
            break;
        }
        let rect = Rect::new(x, area.y, w, area.height.min(3));
        f.render_widget(
            Paragraph::new(label.as_str())
                .alignment(Alignment::Center)
                .style(Style::new().fg(*color).add_modifier(Modifier::BOLD))
                .block(Block::bordered().border_style(Style::new().fg(*color))),
            rect,
        );
        hit(app, rect, *action);
        x += w + 1;
    }
}

fn wallet_buttons(s: &Session) -> Vec<(String, Action, Color)> {
    let busy = s.pending.is_some();
    let mut b: Vec<(String, Action, Color)> = Vec::new();
    match s.tab {
        3 => match &s.send.stage {
            Stage::Editing => b.push(("Build PSBT (Enter)".into(), Action::Build, OK)),
            Stage::Built(_) => {
                b.push(("Sign (g)".into(), Action::Sign, OK));
                b.push(("Cancel (Esc)".into(), Action::CancelPayment, ERR));
            }
            Stage::Signed(_) => {
                b.push(("Broadcast (b)".into(), Action::Broadcast, OK));
                b.push(("Cancel (Esc)".into(), Action::CancelPayment, ERR));
            }
            Stage::Broadcast(_) => b.push(("New payment (Enter)".into(), Action::NewPayment, OK)),
        },
        0 => b.push(("Step sync (f)".into(), Action::SyncStep, ACCENT)),
        1 => b.push(("Copy address (c)".into(), Action::CopyAddress, ACCENT)),
        2 => b.push(("Copy outpoint (c)".into(), Action::CopyOutpoint, ACCENT)),
        4 => b.push(("Copy log (c)".into(), Action::CopyLog, ACCENT)),
        _ => {}
    }
    b.push((if busy { "Syncing...".into() } else { "Sync (s)".to_string() }, Action::Sync, ACCENT));
    b.push(("New address (n)".into(), Action::NewAddress, ACCENT));
    if s.miner.is_some() {
        b.push(("Mine 1 (m)".into(), Action::Mine(1), WARN));
        b.push(("Mine 101 (M)".into(), Action::Mine(101), WARN));
    }
    b.push(("Quit (q)".into(), Action::Quit, ERR));
    b
}

fn input_line<'a>(label: &'a str, input: &'a Input, active: bool) -> Vec<Line<'a>> {
    let style = if active { Style::new().fg(WARN) } else { Style::new() };
    let cursor = if active { "_" } else { "" };
    vec![
        Line::from(Span::styled(label, Style::new().add_modifier(Modifier::DIM))),
        Line::from(Span::styled(format!("> {}{cursor}", input.text), style)),
        Line::default(),
    ]
}

fn panel(title: &str) -> Block<'_> {
    Block::bordered().border_style(Style::new().fg(ACCENT)).title(title)
}

// ---------- setup ----------

fn draw_setup(f: &mut Frame, area: Rect, app: &App, setup: &Setup) {
    let [menu, form] = Layout::vertical([Constraint::Length(5), Constraint::Min(0)]).areas(area);
    let block = panel("Create wallet");
    let inner = block.inner(menu);
    f.render_widget(block.title(format!(" on {} - click or use Up/Down ", app.network)), menu);
    for (i, (_, label)) in SetupMode::ALL.iter().enumerate() {
        let row = Rect::new(inner.x, inner.y + i as u16, inner.width, 1);
        let marker = if i == setup.mode { "(*)" } else { "( )" };
        let style = if i == setup.mode { Style::new().fg(ACCENT).bold() } else { Style::new() };
        f.render_widget(Paragraph::new(format!("{marker} {label}")).style(style), row);
        hit(app, row, Action::SetMode(i));
    }

    let block = panel("Details");
    let inner = block.inner(form);
    let mut lines: Vec<Line> = Vec::new();
    for (i, (label, input)) in setup.fields().into_iter().enumerate() {
        lines.extend(input_line(label, input, i == setup.field));
        hit(app, Rect::new(inner.x, inner.y + 3 * i as u16, inner.width, 2), Action::SetupField(i));
    }
    match setup.mode() {
        SetupMode::NewMnemonic => lines.push(Line::from(
            "Create wallet generates a 12-word recovery phrase and shows it once. Use test networks only.",
        )),
        SetupMode::Descriptor => lines.push(Line::from(
            "e.g. wpkh(tprv.../84h/1h/0h/0/*). Leave internal empty for a single-descriptor wallet.",
        )),
        SetupMode::ImportMnemonic => {}
    }
    f.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }).block(block), form);
}

fn draw_backup(f: &mut Frame, area: Rect, b: &Backup) {
    let words: Vec<&str> = b.words.split_whitespace().collect();
    let mut lines = vec![
        Line::from("Your recovery phrase").bold().fg(WARN),
        Line::from("Anyone with these words controls this wallet. This is the only time it is shown."),
        Line::default(),
    ];
    let rows = words.len().div_ceil(3);
    for r in 0..rows {
        let spans: Vec<Span> = (0..3)
            .filter_map(|c| words.get(r + c * rows).map(|w| (r + c * rows + 1, w)))
            .map(|(n, w)| Span::styled(format!("{n:>2}. {w:<12}"), Style::new().fg(ACCENT).bold()))
            .collect();
        lines.push(Line::from(spans));
    }
    lines.push(Line::default());
    lines.push(Line::from("Copy it with the button below (or press c), store it offline, then continue."));
    f.render_widget(
        Paragraph::new(lines)
            .alignment(Alignment::Center)
            .wrap(Wrap { trim: false })
            .block(Block::bordered().border_style(Style::new().fg(WARN)).title("Back up your wallet")),
        area,
    );
}

// ---------- wallet ----------

fn draw_tabs(f: &mut Frame, area: Rect, app: &App, s: &Session) {
    let title = format!("wallet-lib | {} | {} | {}", s.wallet.network(), s.backend_label, sync_label(s));
    let block = panel(&title);
    let inner = block.inner(area);
    f.render_widget(block, area);
    let mut x = inner.x;
    for (i, name) in TABS.iter().enumerate() {
        let label = format!(" {} {name} ", i + 1);
        let w = label.chars().count() as u16;
        let rect = Rect::new(x, inner.y, w, 1);
        let style = if i == s.tab { Style::new().fg(Color::Black).bg(ACCENT).bold() } else { Style::new() };
        f.render_widget(Paragraph::new(label).style(style), rect);
        hit(app, rect, Action::Tab(i));
        x += w + 1;
    }
}

fn sync_label(s: &Session) -> String {
    match s.next_sync_in() {
        None => "syncing...".into(),
        Some(d) => {
            let secs = d.as_secs();
            format!("next sync in {:02}:{:02}", secs / 60, secs % 60)
        }
    }
}

fn card(f: &mut Frame, area: Rect, title: &str, value: String, color: Color) {
    f.render_widget(
        Paragraph::new(Line::from(value).bold().fg(color))
            .alignment(Alignment::Center)
            .block(Block::bordered().border_style(Style::new().fg(color)).title(title)),
        area,
    );
}

fn draw_overview(f: &mut Frame, area: Rect, app: &App, s: &Session) {
    let b = s.wallet.balance();
    let [cards, lower] = Layout::vertical([Constraint::Length(3), Constraint::Min(0)]).areas(area);
    let [c1, c2, c3] = Layout::horizontal([Constraint::Ratio(1, 3); 3]).areas(cards);
    card(f, c1, "Total", b.total().to_string(), OK);
    card(f, c2, "Spendable", b.spendable().to_string(), ACCENT);
    card(f, c3, "Pending", (b.trusted_pending + b.untrusted_pending).to_string(), WARN);

    let [info, activity] = Layout::horizontal([Constraint::Percentage(45), Constraint::Percentage(55)]).areas(lower);
    let addr = s.address.as_ref().map(|a| a.to_string()).unwrap_or_default();
    let mut lines = vec![
        Line::from(format!("Confirmed   {}", b.confirmed)),
        Line::from(format!("Immature    {}", b.immature)),
        Line::default(),
        Line::from(format!("Network     {}", s.wallet.network())),
        Line::from(format!("Backend     {}", s.backend_label)),
        Line::from(format!("Mining      {}", if s.miner.is_some() { "available (m / M)" } else { "regtest + RPC only" })),
        Line::default(),
    ];
    lines.extend(sync_lines(s, info.width.saturating_sub(2) as usize));
    lines.push(Line::default());
    lines.push(Line::from("Receive address (click to copy):"));
    let addr_row = lines.len() as u16;
    lines.push(Line::from(addr).fg(OK).bold());
    let block = panel("Wallet");
    let inner = block.inner(info);
    f.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }).block(block), info);
    hit(app, Rect::new(inner.x, inner.y + addr_row - 1, inner.width, 2), Action::CopyAddress);

    let block = panel("Recent activity (click for full log)");
    let height = activity.height.saturating_sub(2) as usize;
    let start = app.log.len().saturating_sub(height.max(1));
    let lines: Vec<Line> = app.log[start..].iter().map(|l| log_line(l)).collect();
    f.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }).block(block), activity);
    hit(app, activity, Action::Tab(4));
}

/// Progress bar while a sync runs, otherwise the outcome of the last one.
fn sync_lines(s: &Session, width: usize) -> Vec<Line<'static>> {
    if s.pending.is_some() {
        let (scanned, total) = s
            .progress
            .as_ref()
            .map(|p| (p.scanned.load(Ordering::Relaxed), p.total.load(Ordering::Relaxed)))
            .unwrap_or((0, 0));
        let text = if total > 0 { format!("{scanned}/{total} blocks") } else { "contacting backend".into() };
        let ratio = if total > 0 { scanned as f64 / total as f64 } else { 0.0 };
        return vec![
            Line::from(format!("Refreshing  {text}")).fg(WARN).bold(),
            Line::from(bar(ratio, width.saturating_sub(2).clamp(10, 40))).fg(WARN),
        ];
    }
    let age = s.synced_at.map(|t| format!(" ({}s ago)", t.elapsed().as_secs())).unwrap_or_default();
    let color = if s.sync_ok { OK } else { ERR };
    vec![
        Line::from(format!("Last refresh {}{age}", s.sync_status)).fg(color).bold(),
        Line::from(format!("Next refresh {}", sync_label(s))),
    ]
}

fn bar(ratio: f64, width: usize) -> String {
    let filled = ((ratio.clamp(0.0, 1.0)) * width as f64).round() as usize;
    format!("[{}{}]", "#".repeat(filled), "-".repeat(width - filled))
}

fn draw_receive(f: &mut Frame, area: Rect, app: &App, s: &Session) {
    let addr = s.address.as_ref().map(|a| a.to_string()).unwrap_or_default();
    let qr = qr_lines(&addr);
    let qr_w = qr.iter().map(|l| l.chars().count()).max().unwrap_or(0) as u16 + 4;
    let [text, code] = Layout::horizontal([Constraint::Min(20), Constraint::Length(qr_w.min(area.width / 2))]).areas(area);

    let lines = vec![
        Line::from("Latest receive address (click to copy):"),
        Line::from(addr).bold().fg(OK),
        Line::default(),
        Line::from("Send test coins here, then press Sync."),
        Line::from("New address (n) reveals a fresh one."),
    ];
    let block = panel("Receive");
    let inner = block.inner(text);
    f.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }).block(block), text);
    hit(app, Rect::new(inner.x, inner.y, inner.width, 3), Action::CopyAddress);

    // Dark modules are drawn as black blocks on a white field so phone cameras can read it.
    let qr_lines: Vec<Line> = qr.into_iter().map(Line::from).collect();
    f.render_widget(
        Paragraph::new(qr_lines)
            .alignment(Alignment::Center)
            .style(Style::new().fg(Color::Black).bg(Color::White))
            .block(panel("QR")),
        code,
    );
}

fn qr_lines(addr: &str) -> Vec<String> {
    use qrcode::QrCode;
    use qrcode::render::unicode::Dense1x2;
    // Upper case lets the QR use the compact alphanumeric mode; bech32 is case-insensitive.
    let Ok(code) = QrCode::new(addr.to_uppercase()) else {
        return Vec::new();
    };
    code.render::<Dense1x2>().build().lines().map(str::to_owned).collect()
}

fn draw_utxos(f: &mut Frame, area: Rect, app: &App, s: &Session) {
    let utxos = s.wallet.list_utxos();
    let block = panel("UTXOs - click or Up/Down to select, c to copy outpoint");
    let inner = block.inner(area);
    let visible = inner.height.saturating_sub(1) as usize; // minus header row
    let offset = s.utxo_sel.saturating_sub(visible.saturating_sub(1));
    let rows = utxos.iter().enumerate().skip(offset).take(visible).map(|(i, u)| {
        let row = Row::new(vec![
            u.outpoint.to_string(),
            u.amount.to_string(),
            if u.confirmed { "confirmed" } else { "pending" }.to_string(),
            if u.is_change { "change" } else { "receive" }.to_string(),
        ]);
        if i == s.utxo_sel { row.style(Style::new().fg(Color::Black).bg(ACCENT)) } else { row }
    });
    for (n, i) in (offset..utxos.len()).take(visible).enumerate() {
        hit(app, Rect::new(inner.x, inner.y + 1 + n as u16, inner.width, 1), Action::SelectUtxo(i));
    }
    let table = Table::new(
        rows,
        [Constraint::Min(30), Constraint::Length(18), Constraint::Length(10), Constraint::Length(8)],
    )
    .header(Row::new(vec!["Outpoint", "Amount", "State", "Kind"]).bold())
    .block(block);
    f.render_widget(table, area);
}

fn draw_send(f: &mut Frame, area: Rect, app: &App, s: &Session) {
    let send = &s.send;
    let editing = matches!(send.stage, Stage::Editing);
    let block = panel("Send");
    let inner = block.inner(area);
    let mut lines = Vec::new();
    let fields = [
        ("Recipient address", &send.address, SendField::Address),
        ("Amount (sats)", &send.amount, SendField::Amount),
        ("Fee rate (sat/vB)", &send.fee_rate, SendField::FeeRate),
    ];
    for (i, (label, input, field)) in fields.into_iter().enumerate() {
        lines.extend(input_line(label, input, editing && send.field == field));
        hit(app, Rect::new(inner.x, inner.y + 3 * i as u16, inner.width, 2), Action::SendField(field));
    }
    lines.push(stepper(s));
    lines.push(Line::default());
    let recipient_spk = Address::from_str(send.address.text.trim()).ok().map(|a| a.assume_checked().script_pubkey());
    match &send.stage {
        Stage::Editing => lines.push(Line::from("Click a field or press Tab to switch, then Build PSBT.")),
        Stage::Built(psbt) => {
            lines.push(Line::from("PSBT built - review it, then sign.").bold());
            lines.extend(tx_details(s, &psbt.unsigned_tx, recipient_spk.as_ref(), None));
        }
        Stage::Signed(tx) => {
            lines.push(Line::from("Signed - review it, then broadcast.").bold());
            lines.push(Line::from(format!("Txid        {}", tx.compute_txid())));
            lines.extend(tx_details(s, tx, recipient_spk.as_ref(), Some(tx.vsize())));
        }
        Stage::Broadcast(txid) => {
            lines.push(Line::from(format!("Broadcast {txid}")).fg(OK).bold());
            lines.push(match confirmation(s, txid) {
                Some(true) => Line::from("Confirmed on chain.").fg(OK).bold(),
                Some(false) => Line::from("In the mempool, waiting for a block (regtest: press m, then sync).").fg(WARN),
                None => Line::from("Sent. Sync to see it; confirmation shows once the change output is confirmed.").fg(WARN),
            });
        }
    }
    f.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }).block(block), area);
}

/// `Some(confirmed)` when the wallet holds an output of `txid` (the change output).
fn confirmation(s: &Session, txid: &bitcoin::Txid) -> Option<bool> {
    s.wallet.list_utxos().iter().find(|u| u.outpoint.txid == *txid).map(|u| u.confirmed)
}

/// Build -> Sign -> Broadcast -> Confirm, with finished steps ticked.
fn stepper(s: &Session) -> Line<'static> {
    let done = match &s.send.stage {
        Stage::Editing => 0,
        Stage::Built(_) => 1,
        Stage::Signed(_) => 2,
        Stage::Broadcast(txid) => if confirmation(s, txid) == Some(true) { 4 } else { 3 },
    };
    let mut spans = Vec::new();
    for (i, name) in ["Build", "Sign", "Broadcast", "Confirm"].iter().enumerate() {
        if i > 0 {
            spans.push(Span::raw("  ---  "));
        }
        spans.push(match i.cmp(&done) {
            std::cmp::Ordering::Less => Span::styled(format!("[x] {name}"), Style::new().fg(OK).bold()),
            std::cmp::Ordering::Equal => Span::styled(format!("[>] {name}"), Style::new().fg(WARN).bold()),
            std::cmp::Ordering::Greater => Span::styled(format!("[ ] {name}"), Style::new().add_modifier(Modifier::DIM)),
        });
    }
    Line::from(spans)
}

fn tx_details(s: &Session, tx: &Transaction, recipient: Option<&ScriptBuf>, vsize: Option<usize>) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    let paid: Amount = tx.output.iter().filter(|o| Some(&o.script_pubkey) == recipient).map(|o| o.value).sum();
    lines.push(Line::from(format!("To          {}", s.send.address.text.trim())));
    lines.push(Line::from(format!("Amount      {paid}")).bold());
    if let Some(fee) = s.send.fee {
        let rate = vsize.map(|v| format!(" ({:.1} sat/vB)", fee.to_sat() as f64 / v as f64)).unwrap_or_default();
        lines.push(Line::from(format!("Network fee {fee}{rate}")));
        lines.push(Line::from(format!("Total debit {}", paid + fee)).bold());
    }
    lines.push(Line::from(format!("Inputs      {}", tx.input.len())));
    for out in &tx.output {
        let label = if Some(&out.script_pubkey) == recipient { "recipient" } else { "change" };
        lines.push(Line::from(format!("  output    {} -> {label}", out.value)));
    }
    lines
}

fn log_line(l: &str) -> Line<'_> {
    let color = if is_error(l) {
        ERR
    } else if crate::friendly::is_ok(l) {
        OK
    } else {
        Color::Reset
    };
    Line::from(l).style(Style::new().fg(color))
}

fn draw_log(f: &mut Frame, area: Rect, app: &App) {
    let height = area.height.saturating_sub(2) as usize;
    let end = app.log.len().saturating_sub(app.log_scroll);
    let start = end.saturating_sub(height.max(1));
    let lines: Vec<Line> = app.log[start..end].iter().map(|l| log_line(l)).collect();
    let title = if app.log_scroll > 0 {
        format!("Log - scrolled up {} (End: jump to latest)", app.log_scroll)
    } else {
        "Log - wheel / Up / Down / PageUp / PageDown to scroll".to_string()
    };
    f.render_widget(
        Paragraph::new(lines)
            .wrap(Wrap { trim: false })
            .block(Block::new().borders(Borders::ALL).border_style(Style::new().fg(ACCENT)).title(title)),
        area,
    );
}
