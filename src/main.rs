use anyhow::Context;
use std::{env, fs::File, io};

fn main() -> anyhow::Result<()> {
    let path = env::args()
        .nth(1)
        .context("usage: payments-engine <transactions.csv>")?;

    let file = File::open(&path).with_context(|| format!("cannot open {path}"))?;

    payments_engine::run(file, io::stdout().lock())
}
