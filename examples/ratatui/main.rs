//! A full-featured terminal wallet built on `wallet-lib` and ratatui.
//!
//! ```text
//! cargo run --example ratatui -- --mock                 # offline, fake 100k sat deposit
//! cargo run --example ratatui                           # signet via Esplora
//! cargo run --example ratatui -- --network testnet --esplora https://blockstream.info/testnet/api
//! cargo run --example ratatui -- --network regtest --rpc http://127.0.0.1:18443 --rpc-user polaruser --rpc-pass polarpass
//! ```
//!
//! Covers: create from mnemonic (generate/import) or descriptor, new address, balance,
//! UTXOs, sync (one-shot and step by step), build PSBT, sign, broadcast.
//! Use test networks only.

mod actions;
mod app;
mod backend;
mod clipboard;
mod friendly;
mod rpc_backend;
mod ui;

use std::io;
use std::time::Duration;

use bitcoin::Network;
use crossterm::event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyEventKind};
use crossterm::execute;

use app::{App, BackendChoice};

fn parse_args() -> Result<(Network, BackendChoice), String> {
    let mut network = None;
    let mut esplora = None;
    let mut mock = false;
    // let mut rpc = None;
    let mut rpc = std::env::var("POLAR_RPC_URL").ok();
    let mut rpc_user = std::env::var("POLAR_RPC_USER").unwrap_or_else(|_| "polaruser".to_owned());
    let mut rpc_pass = std::env::var("POLAR_RPC_PASS").unwrap_or_else(|_| "polarpass".to_owned());

    // let mut rpc_user = "polaruser".to_owned();
    // let mut rpc_pass = "polarpass".to_owned();
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--mock" => mock = true,
            "--network" => {
                let v = args.next().ok_or("--network needs a value")?;
                network = Some(v.parse::<Network>().map_err(|e| format!("bad network: {e}"))?);
            }
            "--esplora" => esplora = Some(args.next().ok_or("--esplora needs a URL")?),
            "--rpc" => rpc = Some(args.next().ok_or("--rpc needs a URL")?),
            "--rpc-user" => rpc_user = args.next().ok_or("--rpc-user needs a value")?,
            "--rpc-pass" => rpc_pass = args.next().ok_or("--rpc-pass needs a value")?,
            "-h" | "--help" => return Err("usage: ratatui [--mock] [--network signet|testnet|regtest] [--esplora URL | --rpc URL [--rpc-user U] [--rpc-pass P]]".into()),
            other => return Err(format!("unknown argument {other}")),
        }
    }
    let network = network.unwrap_or(if mock {
        Network::Regtest
    } else {
        Network::Signet
    });
    if network == Network::Bitcoin {
        return Err("refusing to run on mainnet: this is a demo".into());
    }
    let choice = if mock {
        BackendChoice::Mock
    } else if let Some(url) = rpc {
        BackendChoice::Rpc(url, rpc_user, rpc_pass)
    } else {
        let default = match network {
            Network::Testnet => "https://blockstream.info/testnet/api",
            _ => "https://mempool.space/signet/api",
        };
        BackendChoice::Esplora(esplora.unwrap_or_else(|| default.to_owned()))
    };
    Ok((network, choice))
}

fn main() -> io::Result<()> {
    let _ = dotenvy::dotenv();
    let (network, choice) = match parse_args() {
        Ok(v) => v,
        Err(msg) => {
            eprintln!("{msg}");
            std::process::exit(2);
        }
    };
    let mut app = App::new(network, choice);
    let mut terminal = ratatui::init();
    let _ = execute!(io::stdout(), EnableMouseCapture);
    let result = run(&mut terminal, &mut app);
    let _ = execute!(io::stdout(), DisableMouseCapture);
    ratatui::restore();
    result
}

fn run(terminal: &mut ratatui::DefaultTerminal, app: &mut App) -> io::Result<()> {
    while !app.should_quit {
        terminal.draw(|f| ui::draw(f, app))?;
        if event::poll(Duration::from_millis(250))? {
            match event::read()? {
                Event::Key(key) if key.kind == KeyEventKind::Press => app.on_key(key),
                Event::Mouse(m) => app.on_mouse(m),
                _ => {}
            }
        }
        app.tick();
    }
    Ok(())
}
