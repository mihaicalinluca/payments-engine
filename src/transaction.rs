use crate::{account::ClientId, error::ParseError, io::Record};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

#[derive(Deserialize, Debug, Clone, Copy, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum TxKind {
    Deposit,
    #[serde(rename = "withdrawal")]
    Withdraw,
    Dispute,
    Resolve,
    Chargeback,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize, Serialize)]
#[serde(transparent)] // makes serde treat the wrapper as the type itself
pub struct TxId(pub u32);

pub enum Transaction {
    // internal type, we go from Record (csv) to this
    Deposit {
        client: ClientId,
        tx_id: TxId,
        amount: Decimal,
    },
    Withdraw {
        client: ClientId,
        tx_id: TxId,
        amount: Decimal,
    },
    Dispute {
        client: ClientId,
        tx_id: TxId,
    },
    Resolve {
        client: ClientId,
        tx_id: TxId,
    },
    Chargeback {
        client: ClientId,
        tx_id: TxId,
    },
}

impl Transaction {
    pub fn client(&self) -> ClientId {
        match self {
            Transaction::Deposit { client, .. }
            | Transaction::Withdraw { client, .. }
            | Transaction::Dispute { client, .. }
            | Transaction::Resolve { client, .. }
            | Transaction::Chargeback { client, .. } => *client,
        }
    }
}

impl TryFrom<Record> for Transaction {
    type Error = ParseError;

    fn try_from(r: Record) -> Result<Self, Self::Error> {
        let Record {
            kind,
            client,
            tx_id,
            amount,
        } = r;

        match kind {
            TxKind::Deposit | TxKind::Withdraw => {
                if amount.is_none() {
                    return Err(ParseError::NoAmount(tx_id));
                }

                // round first so that an amount below 0.0001 cannot sneak
                // through the positivity check and become a zero-amount tx
                let amount = amount.unwrap().round_dp(4);

                if amount <= Decimal::ZERO {
                    return Err(ParseError::AmountMustBePositive(tx_id));
                }

                Ok(match kind {
                    TxKind::Deposit => Transaction::Deposit {
                        client,
                        tx_id,
                        amount,
                    },
                    _ => Transaction::Withdraw {
                        client,
                        tx_id,
                        amount,
                    },
                })
            }

            TxKind::Dispute | TxKind::Resolve | TxKind::Chargeback => {
                if !amount.is_none() {
                    return Err(ParseError::WrongTransactionKind(tx_id));
                }

                Ok(match kind {
                    TxKind::Dispute => Transaction::Dispute { client, tx_id },
                    TxKind::Resolve => Transaction::Resolve { client, tx_id },
                    _ => Transaction::Chargeback { client, tx_id },
                })
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    fn record(kind: TxKind, amount: Option<Decimal>) -> Record {
        Record {
            kind,
            client: ClientId(7),
            tx_id: TxId(42),
            amount,
        }
    }

    fn parse_err(r: Record) -> Option<ParseError> {
        Transaction::try_from(r).err()
    }

    #[test]
    fn deposit_with_amount_parses() {
        let Ok(Transaction::Deposit {
            client,
            tx_id,
            amount,
        }) = Transaction::try_from(record(TxKind::Deposit, Some(dec!(1.5))))
        else {
            panic!("expected a deposit");
        };
        assert_eq!((client, tx_id, amount), (ClientId(7), TxId(42), dec!(1.5)));
    }

    #[test]
    fn withdrawal_with_amount_parses() {
        let Ok(Transaction::Withdraw {
            client,
            tx_id,
            amount,
        }) = Transaction::try_from(record(TxKind::Withdraw, Some(dec!(2))))
        else {
            panic!("expected a withdrawal");
        };
        assert_eq!((client, tx_id, amount), (ClientId(7), TxId(42), dec!(2)));
    }

    #[test]
    fn deposit_and_withdrawal_require_amount() {
        assert_eq!(
            parse_err(record(TxKind::Deposit, None)),
            Some(ParseError::NoAmount(TxId(42)))
        );
        assert_eq!(
            parse_err(record(TxKind::Withdraw, None)),
            Some(ParseError::NoAmount(TxId(42)))
        );
    }

    #[test]
    fn amount_must_be_positive() {
        for kind in [TxKind::Deposit, TxKind::Withdraw] {
            assert_eq!(
                parse_err(record(kind, Some(dec!(0)))),
                Some(ParseError::AmountMustBePositive(TxId(42)))
            );
            assert_eq!(
                parse_err(record(kind, Some(dec!(-1.5)))),
                Some(ParseError::AmountMustBePositive(TxId(42)))
            );
        }
    }

    #[test]
    fn amount_that_rounds_to_zero_is_rejected() {
        for kind in [TxKind::Deposit, TxKind::Withdraw] {
            assert_eq!(
                parse_err(record(kind, Some(dec!(0.00001)))),
                Some(ParseError::AmountMustBePositive(TxId(42)))
            );
        }
    }

    #[test]
    fn amount_is_rounded_to_four_decimal_places() {
        let Ok(Transaction::Deposit { amount, .. }) =
            Transaction::try_from(record(TxKind::Deposit, Some(dec!(1.23456))))
        else {
            panic!("expected a deposit");
        };
        assert_eq!(amount, dec!(1.2346));
    }

    #[test]
    fn dispute_resolve_chargeback_without_amount_parse() {
        assert!(matches!(
            Transaction::try_from(record(TxKind::Dispute, None)),
            Ok(Transaction::Dispute {
                client: ClientId(7),
                tx_id: TxId(42)
            })
        ));
        assert!(matches!(
            Transaction::try_from(record(TxKind::Resolve, None)),
            Ok(Transaction::Resolve {
                client: ClientId(7),
                tx_id: TxId(42)
            })
        ));
        assert!(matches!(
            Transaction::try_from(record(TxKind::Chargeback, None)),
            Ok(Transaction::Chargeback {
                client: ClientId(7),
                tx_id: TxId(42)
            })
        ));
    }

    #[test]
    fn dispute_resolve_chargeback_with_amount_are_rejected() {
        for kind in [TxKind::Dispute, TxKind::Resolve, TxKind::Chargeback] {
            assert_eq!(
                parse_err(record(kind, Some(dec!(1)))),
                Some(ParseError::WrongTransactionKind(TxId(42)))
            );
        }
    }

    #[test]
    fn client_is_returned_for_every_variant() {
        let c = ClientId(9);
        let t = TxId(1);
        let amount = dec!(1);
        let all = [
            Transaction::Deposit {
                client: c,
                tx_id: t,
                amount,
            },
            Transaction::Withdraw {
                client: c,
                tx_id: t,
                amount,
            },
            Transaction::Dispute {
                client: c,
                tx_id: t,
            },
            Transaction::Resolve {
                client: c,
                tx_id: t,
            },
            Transaction::Chargeback {
                client: c,
                tx_id: t,
            },
        ];
        for tx in &all {
            assert_eq!(tx.client(), c);
        }
    }

    /// Same reader settings as `crate::run`.
    fn read_one(csv_text: &str) -> Record {
        csv::ReaderBuilder::new()
            .trim(csv::Trim::All)
            .flexible(true)
            .from_reader(csv_text.as_bytes())
            .deserialize::<Record>()
            .next()
            .expect("one row")
            .expect("row deserializes")
    }

    #[test]
    fn record_deserializes_withdrawal_kind() {
        let r = read_one("type, client, tx, amount\nwithdrawal, 1, 2, 1.5\n");
        assert_eq!(r.kind, TxKind::Withdraw);
        assert_eq!(r.client, ClientId(1));
        assert_eq!(r.tx_id, TxId(2));
        assert_eq!(r.amount, Some(dec!(1.5)));
    }

    #[test]
    fn record_without_amount_column_has_none_amount() {
        let r = read_one("type, client, tx, amount\ndispute,1,1\n");
        assert_eq!(r.kind, TxKind::Dispute);
        assert_eq!(r.amount, None);
    }
}
