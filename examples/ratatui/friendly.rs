//! Plain-language wording for the status bar and activity log.
//!
//! Messages start with a marker so the UI can colour them: success, problem, or neutral info.

pub const OK: &str = "\u{2714} ";
pub const BAD: &str = "\u{2716} ";
pub const INFO: &str = "\u{2022} ";

pub fn ok(msg: impl std::fmt::Display) -> String {
    format!("{OK}{msg}")
}

pub fn bad(msg: impl std::fmt::Display) -> String {
    format!("{BAD}{msg}")
}

pub fn info(msg: impl std::fmt::Display) -> String {
    format!("{INFO}{msg}")
}

pub fn is_bad(msg: &str) -> bool {
    msg.starts_with(BAD)
}

pub fn is_ok(msg: &str) -> bool {
    msg.starts_with(OK)
}

/// Turns a technical error into advice a non-technical user can act on.
/// The original text is kept at the end for anyone helping them.
pub fn explain(raw: &str) -> String {
    let l = raw.to_lowercase();
    let advice = if l.contains("connection refused") || l.contains("could not connect") || l.contains("unreachable") {
        "Cannot reach your Bitcoin node. Check that Polar (or your node) is running and that the address in your settings is right."
    } else if l.contains("401") || l.contains("unauthorized") || l.contains("authentication") {
        "The node refused the username or password. Check the credentials in your settings."
    } else if l.contains("timed out") || l.contains("timeout") {
        "The node took too long to answer. Try again in a moment."
    } else if l.contains("insufficient funds") {
        "You do not have enough spendable coins for this amount plus the network fee. Coins from mining stay locked for 100 blocks."
    } else if l.contains("invalid address") || l.contains("not valid for") || l.contains("network") && l.contains("address") {
        "That address does not belong to this network. Check that you copied it from the right wallet."
    } else if l.contains("missingorspent") || l.contains("missing inputs") {
        "One of the coins was already spent. Refresh your balance and try again."
    } else if l.contains("min relay fee") || l.contains("fee too low") || l.contains("insufficient fee") {
        "The fee is too low for the network to accept. Raise the fee rate and try again."
    } else if l.contains("dust") {
        "The amount is too small to send."
    } else if l.contains("already in") || l.contains("txn-already") {
        "The network already has this payment."
    } else {
        "Something went wrong."
    };
    let detail: String = raw.chars().take(140).collect();
    format!("{advice} (details: {detail})")
}
