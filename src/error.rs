use crate::account::ClientId;
use crate::transaction::TxId;

#[derive(Debug, thiserror::Error, PartialEq)]
pub enum AccountError {
    #[error("insufficient available funds")]
    InsufficientFunds,
    #[error("arithmetic overflow")]
    Overflow,
}

#[derive(Debug, thiserror::Error, PartialEq)]
pub enum TxError {
    #[error("account {0:?} is locked")]
    Locked(ClientId),
    #[error("tx {0:?} already exists")]
    DuplicateTx(TxId),
    #[error("tx {0:?} not found")]
    UnknownTx(TxId),
    #[error("tx {0:?} is not a deposit")]
    NotDisputable(TxId),
    #[error("tx {0:?} belongs to another client")]
    ClientMismatch(TxId),
    #[error("tx {0:?} is not in the right state")]
    WrongState(TxId),

    #[error(transparent)]
    Account(#[from] AccountError),
}

#[derive(Debug, thiserror::Error, PartialEq)]
pub enum ParseError {
    #[error("tx {0:?} requires an amount")]
    NoAmount(TxId),
    #[error("tx {0:?} amount must be positive")]
    AmountMustBePositive(TxId),
    #[error("tx {0:?} must not carry an amount because of its kind")]
    WrongTransactionKind(TxId),
}
