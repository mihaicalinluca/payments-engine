# payments-engine

A small payments engine. It reads a CSV of transactions (deposits, withdrawals,
disputes, resolves, chargebacks), applies them to client accounts, and writes
the final account balances as CSV to stdout.

## Running it

```sh
cargo build
cargo run -- transactions.csv > accounts.csv
cargo test
```

### Input

A CSV file with a header row and the columns `type, client, tx, amount`:

- `type`: one of `deposit`, `withdrawal`, `dispute`, `resolve`, `chargeback`
- `client`: client id, `u16`
- `tx`: transaction id, `u32`
- `amount`: decimal, up to four decimal places; required for deposits and
  withdrawals, absent for the other three kinds

Whitespace around fields is ignored. Rows for `dispute`, `resolve` and
`chargeback` may omit the amount column entirely (`dispute,1,1`) or leave it
empty (`dispute,1,1,`).

### Output

One row per client, ordered by client id, always preceded by a header row:

```
client,available,held,total,locked
```

Amounts are printed with exactly four decimal places.

### Example

Input ([tests/data/sample.csv](tests/data/sample.csv)):

```
type, client, tx, amount
deposit, 1, 1, 1.0
deposit, 2, 2, 2.0
deposit, 1, 3, 2.0
withdrawal, 1, 4, 1.5
withdrawal, 2, 5, 3.0
```

Output ([tests/data/sample.expected.csv](tests/data/sample.expected.csv)):

```
client,available,held,total,locked
1,1.5000,0.0000,1.5000,false
2,2.0000,0.0000,2.0000,false
```

The last withdrawal is rejected because client 2 has only 2.0 available.

### Errors and exit code

Rows that cannot be read, fail validation, or are rejected by the engine are
reported on stderr (with their line number and reason) and skipped. They never
affect stdout beyond not being applied.

The exit code is 0 unless no input path is given, the input file cannot be
opened, or writing to stdout fails.

## How it works

| File | Role |
| --- | --- |
| `src/main.rs` | CLI: reads the path argument, opens the file, calls `run`. |
| `src/lib.rs` | `run` streams records from the CSV reader, converts each to a `Transaction`, feeds it to `State`, then writes the output. |
| `src/io.rs` | `Record` (raw input row), `Output` (output row) and `write_accounts`. |
| `src/transaction.rs` | `TxKind`, `TxId`, and the validated `Transaction` enum with `TryFrom<Record>`. |
| `src/state.rs` | `State`: the accounts map, the transactions map, one handler per transaction kind, and the `TxState` machine for deposits (`Undisputed` / `Disputed` / `ChargedBack`). |
| `src/account.rs` | `Account` and `ClientId`: balance arithmetic. `total` is derived as `available + held`, never stored. |
| `src/error.rs` | The three error enums: `ParseError`, `TxError`, `AccountError`. |

The input side follows "parse, don't validate". `Record` is the raw CSV shape
(the amount is an `Option`, any kind may carry one). `Transaction::try_from`
checks the per-kind rules once and produces a `Transaction` in which deposits
and withdrawals always have a positive amount and the other kinds have none.
`State` only ever sees `Transaction`, so it does not re-check row shape.

## Transaction semantics and assumptions

- Only deposits can be disputed. Disputing a withdrawal is rejected.
- A dispute, resolve or chargeback must come from the client that owns the
  referenced deposit; otherwise it is rejected and the owner's account is not
  touched.
- A dispute moves the deposit amount from available to held; total is unchanged.
- Resolve and chargeback are only valid on a deposit that is currently disputed.
- A resolve moves the amount back from held to available.
- A resolved deposit can be disputed again.
- A chargeback removes the held amount (lowering held and total) and locks the
  account. A charged-back deposit is final and cannot be disputed or resolved.
- Disputing a deposit whose funds were already withdrawn still holds the full
  amount, which can drive available negative (and, after a chargeback, total).
  This is deliberate: the loss stays visible in the output instead of being
  hidden or clamped.
- A withdrawal is rejected when available is less than the amount, which
  includes the case where available is negative.
- Transaction ids must be unique across deposits and withdrawals; a repeated id
  is rejected. Disputes, resolves and chargebacks reference an existing id and
  do not consume one.
- Amounts are rounded to four decimal places when parsed.
- Deposit and withdrawal rows must carry an amount that is still positive after
  rounding, so `0.00001` is rejected rather than recorded as a zero-amount
  transaction that consumes its id.
- Dispute, resolve and chargeback rows must not carry an amount. A row that
  does is rejected, not processed with the amount ignored: the engine cannot
  tell whether the sender meant to dispute the stored deposit amount or the
  one written in the row, and it will not guess on a money-moving action.
- The CSV reader is flexible about row width so that the documented
  three-column form (`dispute,1,1`) is accepted; a consequence is that extra
  trailing columns beyond `amount` pass through unread. Shape is enforced on
  the four documented fields, not on columns past them.
- Unknown transaction types and malformed rows are skipped with a message on
  stderr.
- A withdrawal that fails for a client with no prior activity still creates
  that client's account record, with zero balances, so it appears in the output.
- `total` is always computed as `available + held`, so the invariant cannot
  drift between the fields.

## Design decisions

**Locked accounts are fully frozen.** After a chargeback I reject every further
transaction for that client, including resolves and chargebacks of other open
disputes. I prefer a hard freeze that a human has to look into over leaving any
path where funds move on a frozen account. Tooling to unfreeze an account or
finish its open disputes can be added later as an explicit operation.

**`HashMap` for accounts and transactions, on purpose.** Lookups by client id
and transaction id are the hot path and are O(1) with a `HashMap`. Ordering is
only needed once, at output time, where the accounts (at most 65,536, since
client ids are `u16`) are collected and sorted. A `BTreeMap` would pay for
ordering on every row to save that one sort, so I did not use it. Withdrawals
are kept in the transactions map only so duplicate transaction ids can be
detected; they carry no data.

**Always writing the header row.** An empty or header-only input still produces
the header line. The `csv` writer only emits the header together with the first
serialized record, so I configure it with `has_headers(false)` and write the
header row by hand.

**Exact decimal arithmetic.** Amounts are `rust_decimal::Decimal`, not floats.
Balance changes that can overflow use checked arithmetic and surface
`AccountError::Overflow` instead of panicking or wrapping. `ClientId` and
`TxId` are newtypes so the two kinds of id cannot be mixed up.

## Error handling

Errors are split into three enums, all built with `thiserror`:

- `ParseError`: the row is well-formed CSV but not a valid transaction (missing
  amount, non-positive amount, amount on a dispute/resolve/chargeback).
- `TxError`: the transaction is valid but cannot be applied to the current
  state (locked account, duplicate id, unknown id, not a deposit, wrong client,
  wrong dispute state). It wraps `AccountError`.
- `AccountError`: a balance operation failed (insufficient funds, overflow).

`anyhow` is only used at the CLI boundary (`main` and `run`) for errors that
stop the program: no argument, unopenable file, failed output. Per-row failures
are logged to stderr and skipped; they are never fatal.

## Efficiency

Input is streamed with `csv::Reader::deserialize`, so the file is never loaded
into memory. Each row uses constant memory beyond the two maps. Memory grows
with the number of clients and with the number of deposits (kept because they
can be disputed) and withdrawals (kept for duplicate-id detection).

To handle many concurrent streams: clients are independent of each other, so
each stream can have its own `State`, or a single input can be sharded by client
id across workers. The only cross-client coupling is that transaction ids are
globally unique, which a sharded setup would need to enforce separately (or
relax, if ids are only unique per client).

Possible next steps I deliberately did not take, since nothing here needs them
yet: a compact seen-set (for example a bitmap over the `u32` id space) for
withdrawal dedup instead of map entries, and fixed-point integer amounts
instead of `Decimal`.

## Testing

- Unit tests in `src/state.rs` cover the engine rules: balances, duplicate ids,
  the dispute state machine, locking, negative balances.
- Unit tests in `src/transaction.rs` cover `Record` to `Transaction` validation,
  rounding, and CSV deserialization of `Record`.
- CLI tests in `tests/cli.rs` run the built binary on fixtures in `tests/data/`
  (`<name>.csv` input, `<name>.expected.csv` expected output where applicable)
  and check stdout, stderr and exit code.

Run everything with:

```sh
cargo test
```
