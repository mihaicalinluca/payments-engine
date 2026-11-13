pub mod account;
pub mod error;
pub mod io;
pub mod state;
pub mod transaction;

use crate::{
    io::{Record, write_accounts},
    state::State,
    transaction::Transaction,
};
use std::io::{Read, Write};

pub fn run<R: Read, W: Write>(input: R, output: W) -> anyhow::Result<()> {
    let mut rdr = csv::ReaderBuilder::new()
        .trim(csv::Trim::All)
        .flexible(true)
        .from_reader(input);

    let mut state = State::default();

    for (i, row) in rdr.deserialize::<Record>().enumerate() {
        let line = i + 2; // header is line 1

        let record = match row {
            Ok(r) => r,
            Err(e) => {
                eprintln!("line {line}: skipped, unreadable row: {e}");
                continue;
            }
        };

        // map record to tx
        let tx = match Transaction::try_from(record) {
            Ok(t) => t,
            Err(e) => {
                eprintln!("line {line}: skipped, invalid transaction: {e}");
                continue;
            }
        };

        // process each tx
        if let Err(e) = state.process(tx) {
            eprintln!("line {line}: ignored: {e}");
        }
    }

    // output
    write_accounts(&state, output)?;
    Ok(())
}
