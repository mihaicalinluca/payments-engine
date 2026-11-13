use crate::account::ClientId;
use crate::transaction::{TxId, TxKind};
use crate::{account::Account, state::State};
use rust_decimal::Decimal;
use serde::Deserialize;
use serde::Serialize;
use std::io::Write;

// input
#[derive(Copy, Clone, Deserialize)]
pub struct Record {
    #[serde(rename = "type")]
    pub kind: TxKind,
    pub client: ClientId,
    #[serde(rename = "tx")]
    pub tx_id: TxId,
    pub amount: Option<Decimal>, // missing or empty is None
}

// output
#[derive(Serialize)]
struct Output {
    client: ClientId,
    available: String,
    held: String,
    total: String,
    locked: bool,
}

impl Output {
    fn from_account(client: ClientId, account: &Account) -> Self {
        Self {
            client,
            available: format!("{:.4}", account.available()),
            held: format!("{:.4}", account.held()),
            total: format!("{:.4}", account.total()),
            locked: account.is_locked(),
        }
    }
}

pub fn write_accounts<W: Write>(state: &State, out: W) -> csv::Result<()> {
    let mut wtr = csv::WriterBuilder::new()
        .has_headers(false)
        .from_writer(out);

    wtr.write_record(["client", "available", "held", "total", "locked"])?;

    let mut accounts: Vec<_> = state.accounts().collect();
    accounts.sort_by_key(|(client, _)| *client);

    for (client, account) in accounts {
        wtr.serialize(Output::from_account(client, account))?;
    }

    wtr.flush()?;

    Ok(())
}
