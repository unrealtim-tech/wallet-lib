//! Everything the user can do. Keys and mouse clicks both turn into an [`Action`].

use crate::app::SendField;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Action {
    // setup
    SetMode(usize),
    SetupField(usize),
    Create,
    // recovery phrase screen
    CopyMnemonic,
    ConfirmBackup,
    // wallet
    Tab(usize),
    Sync,
    SyncStep,
    NewAddress,
    Mine(u64),
    CopyAddress,
    CopyOutpoint,
    CopyLog,
    SelectUtxo(usize),
    SendField(SendField),
    Build,
    Sign,
    Broadcast,
    CancelPayment,
    NewPayment,
    Quit,
}
