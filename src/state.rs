use rust_decimal::Decimal;

use crate::{
    account::{Account, ClientId},
    error::TxError,
    transaction::{Transaction, TxId},
};
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TxState {
    Undisputed,
    Disputed,
    ChargedBack,
}

#[derive(Debug)]
pub enum TxInfo {
    Deposit {
        client: ClientId,
        amount: Decimal,
        state: TxState,
    },
    Withdrawal,
}

#[derive(Debug, Default)]
pub struct State {
    accounts: HashMap<ClientId, Account>,
    transactions: HashMap<TxId, TxInfo>,
}

impl State {
    // entry point
    pub fn process(&mut self, tx: Transaction) -> Result<(), TxError> {
        // read client
        // refuse if locked
        // pass to handlers

        let client = tx.client();
        if self.accounts.get(&client).is_some_and(Account::is_locked) {
            return Err(TxError::Locked(client));
        }

        match tx {
            Transaction::Deposit {
                client,
                tx_id,
                amount,
            } => self.deposit(client, tx_id, amount),
            Transaction::Withdraw {
                client,
                tx_id,
                amount,
            } => self.withdraw(client, tx_id, amount),
            Transaction::Dispute { client, tx_id } => self.dispute(client, tx_id),
            Transaction::Resolve { client, tx_id } => self.resolve(client, tx_id),
            Transaction::Chargeback { client, tx_id } => self.chargeback(client, tx_id),
        }
    }

    pub fn accounts(&self) -> impl Iterator<Item = (ClientId, &Account)> {
        self.accounts.iter().map(|(id, acc)| (*id, acc))
    }

    // handlers
    fn deposit(&mut self, client: ClientId, tx_id: TxId, amount: Decimal) -> Result<(), TxError> {
        self.tx_not_duplicate(tx_id)?;

        self.accounts.entry(client).or_default().deposit(amount)?;
        self.transactions.insert(
            tx_id,
            TxInfo::Deposit {
                client,
                amount,
                state: TxState::Undisputed,
            },
        );

        Ok(())
    }

    fn withdraw(&mut self, client: ClientId, tx_id: TxId, amount: Decimal) -> Result<(), TxError> {
        self.tx_not_duplicate(tx_id)?;

        self.accounts.entry(client).or_default().withdraw(amount)?;
        self.transactions.insert(tx_id, TxInfo::Withdrawal);

        Ok(())
    }

    fn dispute(&mut self, client: ClientId, tx_id: TxId) -> Result<(), TxError> {
        let (amount, state) = find_deposit(&mut self.transactions, client, tx_id)?;

        if *state != TxState::Undisputed {
            return Err(TxError::WrongState(tx_id));
        }

        self.accounts.entry(client).or_default().hold(amount)?;
        *state = TxState::Disputed;

        Ok(())
    }

    fn resolve(&mut self, client: ClientId, tx_id: TxId) -> Result<(), TxError> {
        let (amount, state) = find_deposit(&mut self.transactions, client, tx_id)?;

        if *state != TxState::Disputed {
            return Err(TxError::WrongState(tx_id));
        }

        self.accounts.entry(client).or_default().release(amount)?;
        *state = TxState::Undisputed;

        Ok(())
    }

    fn chargeback(&mut self, client: ClientId, tx_id: TxId) -> Result<(), TxError> {
        let (amount, state) = find_deposit(&mut self.transactions, client, tx_id)?;

        if *state != TxState::Disputed {
            return Err(TxError::WrongState(tx_id));
        }

        self.accounts
            .entry(client)
            .or_default()
            .charge_back(amount)?;
        *state = TxState::ChargedBack;

        Ok(())
    }

    // helpers
    fn tx_not_duplicate(&self, tx_id: TxId) -> Result<(), TxError> {
        if self.transactions.contains_key(&tx_id) {
            return Err(TxError::DuplicateTx(tx_id));
        }

        Ok(())
    }
}

fn find_deposit(
    transactions: &mut HashMap<TxId, TxInfo>,
    client: ClientId,
    tx_id: TxId,
) -> Result<(Decimal, &mut TxState), TxError> {
    match transactions.get_mut(&tx_id) {
        None => Err(TxError::UnknownTx(tx_id)),
        Some(TxInfo::Withdrawal) => Err(TxError::NotDisputable(tx_id)),
        Some(TxInfo::Deposit { client: owner, .. }) if *owner != client => {
            Err(TxError::ClientMismatch(tx_id))
        }
        Some(TxInfo::Deposit { amount, state, .. }) => Ok((*amount, state)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::AccountError;
    use rust_decimal_macros::dec;

    const A: ClientId = ClientId(1);
    const B: ClientId = ClientId(2);

    fn deposit(client: ClientId, tx: u32, amount: Decimal) -> Transaction {
        Transaction::Deposit {
            client,
            tx_id: TxId(tx),
            amount,
        }
    }

    fn withdraw(client: ClientId, tx: u32, amount: Decimal) -> Transaction {
        Transaction::Withdraw {
            client,
            tx_id: TxId(tx),
            amount,
        }
    }

    fn dispute(client: ClientId, tx: u32) -> Transaction {
        Transaction::Dispute {
            client,
            tx_id: TxId(tx),
        }
    }

    fn resolve(client: ClientId, tx: u32) -> Transaction {
        Transaction::Resolve {
            client,
            tx_id: TxId(tx),
        }
    }

    fn chargeback(client: ClientId, tx: u32) -> Transaction {
        Transaction::Chargeback {
            client,
            tx_id: TxId(tx),
        }
    }

    /// (available, held, total, locked) for `client`; panics if the account does not exist.
    fn balances(state: &State, client: ClientId) -> (Decimal, Decimal, Decimal, bool) {
        let (_, acc) = state
            .accounts()
            .find(|(id, _)| *id == client)
            .expect("account should exist");
        (acc.available(), acc.held(), acc.total(), acc.is_locked())
    }

    fn has_account(state: &State, client: ClientId) -> bool {
        state.accounts().any(|(id, _)| id == client)
    }

    #[test]
    fn deposit_creates_account_and_credits_available() {
        let mut s = State::default();
        assert_eq!(s.process(deposit(A, 1, dec!(1.5))), Ok(()));
        assert_eq!(balances(&s, A), (dec!(1.5), dec!(0), dec!(1.5), false));
    }

    #[test]
    fn deposits_accumulate() {
        let mut s = State::default();
        s.process(deposit(A, 1, dec!(1.5))).unwrap();
        s.process(deposit(A, 2, dec!(2.25))).unwrap();
        assert_eq!(balances(&s, A), (dec!(3.75), dec!(0), dec!(3.75), false));
    }

    #[test]
    fn withdrawal_lowers_available_and_total() {
        let mut s = State::default();
        s.process(deposit(A, 1, dec!(5))).unwrap();
        assert_eq!(s.process(withdraw(A, 2, dec!(1.5))), Ok(()));
        assert_eq!(balances(&s, A), (dec!(3.5), dec!(0), dec!(3.5), false));
    }

    #[test]
    fn withdrawal_with_insufficient_funds_is_rejected() {
        let mut s = State::default();
        s.process(deposit(A, 1, dec!(1))).unwrap();
        assert_eq!(
            s.process(withdraw(A, 2, dec!(1.0001))),
            Err(TxError::Account(AccountError::InsufficientFunds))
        );
        assert_eq!(balances(&s, A), (dec!(1), dec!(0), dec!(1), false));
    }

    #[test]
    fn failed_withdrawal_for_new_client_creates_empty_account() {
        let mut s = State::default();
        assert_eq!(
            s.process(withdraw(A, 1, dec!(1))),
            Err(TxError::Account(AccountError::InsufficientFunds))
        );
        assert_eq!(balances(&s, A), (dec!(0), dec!(0), dec!(0), false));
    }

    #[test]
    fn duplicate_deposit_id_is_rejected() {
        let mut s = State::default();
        s.process(deposit(A, 1, dec!(1))).unwrap();
        assert_eq!(
            s.process(deposit(A, 1, dec!(2))),
            Err(TxError::DuplicateTx(TxId(1)))
        );
        assert_eq!(balances(&s, A), (dec!(1), dec!(0), dec!(1), false));
    }

    #[test]
    fn withdrawal_reusing_deposit_id_is_rejected() {
        let mut s = State::default();
        s.process(deposit(A, 1, dec!(5))).unwrap();
        assert_eq!(
            s.process(withdraw(A, 1, dec!(1))),
            Err(TxError::DuplicateTx(TxId(1)))
        );
        assert_eq!(balances(&s, A), (dec!(5), dec!(0), dec!(5), false));
    }

    #[test]
    fn withdrawal_reusing_withdrawal_id_is_rejected() {
        let mut s = State::default();
        s.process(deposit(A, 1, dec!(5))).unwrap();
        s.process(withdraw(A, 2, dec!(1))).unwrap();
        assert_eq!(
            s.process(withdraw(A, 2, dec!(1))),
            Err(TxError::DuplicateTx(TxId(2)))
        );
        assert_eq!(balances(&s, A), (dec!(4), dec!(0), dec!(4), false));
    }

    #[test]
    fn dispute_moves_funds_to_held() {
        let mut s = State::default();
        s.process(deposit(A, 1, dec!(3))).unwrap();
        s.process(deposit(A, 2, dec!(2))).unwrap();
        assert_eq!(s.process(dispute(A, 1)), Ok(()));
        assert_eq!(balances(&s, A), (dec!(2), dec!(3), dec!(5), false));
    }

    #[test]
    fn dispute_on_unknown_tx_is_rejected() {
        let mut s = State::default();
        s.process(deposit(A, 1, dec!(3))).unwrap();
        assert_eq!(s.process(dispute(A, 99)), Err(TxError::UnknownTx(TxId(99))));
        assert_eq!(balances(&s, A), (dec!(3), dec!(0), dec!(3), false));
    }

    #[test]
    fn dispute_on_withdrawal_is_rejected() {
        let mut s = State::default();
        s.process(deposit(A, 1, dec!(3))).unwrap();
        s.process(withdraw(A, 2, dec!(1))).unwrap();
        assert_eq!(
            s.process(dispute(A, 2)),
            Err(TxError::NotDisputable(TxId(2)))
        );
        assert_eq!(balances(&s, A), (dec!(2), dec!(0), dec!(2), false));
    }

    #[test]
    fn dispute_on_other_clients_deposit_is_rejected() {
        let mut s = State::default();
        s.process(deposit(A, 1, dec!(3))).unwrap();
        assert_eq!(
            s.process(dispute(B, 1)),
            Err(TxError::ClientMismatch(TxId(1)))
        );
        assert_eq!(balances(&s, A), (dec!(3), dec!(0), dec!(3), false));
        assert!(!has_account(&s, B));
    }

    #[test]
    fn dispute_twice_is_rejected() {
        let mut s = State::default();
        s.process(deposit(A, 1, dec!(3))).unwrap();
        s.process(dispute(A, 1)).unwrap();
        assert_eq!(s.process(dispute(A, 1)), Err(TxError::WrongState(TxId(1))));
        assert_eq!(balances(&s, A), (dec!(0), dec!(3), dec!(3), false));
    }

    #[test]
    fn resolve_releases_held_funds() {
        let mut s = State::default();
        s.process(deposit(A, 1, dec!(3))).unwrap();
        s.process(dispute(A, 1)).unwrap();
        assert_eq!(s.process(resolve(A, 1)), Ok(()));
        assert_eq!(balances(&s, A), (dec!(3), dec!(0), dec!(3), false));
    }

    #[test]
    fn resolve_on_undisputed_tx_is_rejected() {
        let mut s = State::default();
        s.process(deposit(A, 1, dec!(3))).unwrap();
        assert_eq!(s.process(resolve(A, 1)), Err(TxError::WrongState(TxId(1))));
        assert_eq!(balances(&s, A), (dec!(3), dec!(0), dec!(3), false));
    }

    #[test]
    fn resolved_tx_can_be_disputed_again() {
        let mut s = State::default();
        s.process(deposit(A, 1, dec!(3))).unwrap();
        s.process(dispute(A, 1)).unwrap();
        s.process(resolve(A, 1)).unwrap();
        assert_eq!(s.process(dispute(A, 1)), Ok(()));
        assert_eq!(balances(&s, A), (dec!(0), dec!(3), dec!(3), false));
    }

    #[test]
    fn chargeback_removes_held_funds_and_locks() {
        let mut s = State::default();
        s.process(deposit(A, 1, dec!(3))).unwrap();
        s.process(deposit(A, 2, dec!(2))).unwrap();
        s.process(dispute(A, 1)).unwrap();
        assert_eq!(s.process(chargeback(A, 1)), Ok(()));
        assert_eq!(balances(&s, A), (dec!(2), dec!(0), dec!(2), true));
    }

    #[test]
    fn chargeback_on_undisputed_tx_is_rejected() {
        let mut s = State::default();
        s.process(deposit(A, 1, dec!(3))).unwrap();
        assert_eq!(
            s.process(chargeback(A, 1)),
            Err(TxError::WrongState(TxId(1)))
        );
        assert_eq!(balances(&s, A), (dec!(3), dec!(0), dec!(3), false));
    }

    #[test]
    fn charged_back_tx_cannot_be_resolved_or_disputed() {
        let mut s = State::default();
        s.process(deposit(A, 1, dec!(3))).unwrap();
        s.process(dispute(A, 1)).unwrap();
        s.process(chargeback(A, 1)).unwrap();

        // Through `process` the lock check fires first, since only the owner
        // (now locked) may reference the tx.
        assert_eq!(s.process(resolve(A, 1)), Err(TxError::Locked(A)));
        assert_eq!(s.process(dispute(A, 1)), Err(TxError::Locked(A)));

        // The tx state machine itself also treats ChargedBack as final.
        assert_eq!(s.resolve(A, TxId(1)), Err(TxError::WrongState(TxId(1))));
        assert_eq!(s.dispute(A, TxId(1)), Err(TxError::WrongState(TxId(1))));
        assert_eq!(s.chargeback(A, TxId(1)), Err(TxError::WrongState(TxId(1))));
        assert_eq!(balances(&s, A), (dec!(0), dec!(0), dec!(0), true));
    }

    #[test]
    fn locked_account_rejects_every_transaction() {
        let mut s = State::default();
        s.process(deposit(A, 1, dec!(3))).unwrap();
        s.process(deposit(A, 2, dec!(2))).unwrap();
        s.process(dispute(A, 1)).unwrap();
        s.process(dispute(A, 2)).unwrap();
        s.process(chargeback(A, 1)).unwrap();
        let before = balances(&s, A);
        assert_eq!(before, (dec!(0), dec!(2), dec!(2), true));

        let locked = Err(TxError::Locked(A));
        assert_eq!(s.process(deposit(A, 3, dec!(1))), locked);
        assert_eq!(s.process(withdraw(A, 4, dec!(1))), locked);
        assert_eq!(s.process(dispute(A, 2)), locked);
        assert_eq!(s.process(resolve(A, 2)), locked);
        assert_eq!(s.process(chargeback(A, 2)), locked);
        assert_eq!(balances(&s, A), before);
    }

    #[test]
    fn dispute_after_withdrawal_can_drive_available_negative() {
        let mut s = State::default();
        s.process(deposit(A, 1, dec!(10))).unwrap();
        s.process(withdraw(A, 2, dec!(8))).unwrap();

        assert_eq!(s.process(dispute(A, 1)), Ok(()));
        assert_eq!(balances(&s, A), (dec!(-8), dec!(10), dec!(2), false));

        assert_eq!(
            s.process(withdraw(A, 3, dec!(1))),
            Err(TxError::Account(AccountError::InsufficientFunds))
        );
        assert_eq!(balances(&s, A), (dec!(-8), dec!(10), dec!(2), false));

        assert_eq!(s.process(chargeback(A, 1)), Ok(()));
        assert_eq!(balances(&s, A), (dec!(-8), dec!(0), dec!(-8), true));
    }

    #[test]
    fn chargeback_locks_only_its_own_client() {
        let mut s = State::default();
        s.process(deposit(A, 1, dec!(3))).unwrap();
        s.process(deposit(B, 2, dec!(4))).unwrap();
        s.process(dispute(A, 1)).unwrap();
        s.process(chargeback(A, 1)).unwrap();

        assert!(balances(&s, A).3);
        assert_eq!(balances(&s, B), (dec!(4), dec!(0), dec!(4), false));
        assert_eq!(s.process(deposit(B, 3, dec!(1))), Ok(()));
        assert_eq!(balances(&s, B), (dec!(5), dec!(0), dec!(5), false));
    }

    #[test]
    fn accounts_yields_every_touched_client() {
        let mut s = State::default();
        s.process(deposit(ClientId(1), 1, dec!(1))).unwrap();
        s.process(deposit(ClientId(2), 2, dec!(1))).unwrap();
        // failed withdrawal still creates the account record
        let _ = s.process(withdraw(ClientId(3), 3, dec!(1)));

        let mut ids: Vec<u16> = s.accounts().map(|(id, _)| id.0).collect();
        ids.sort_unstable();
        assert_eq!(ids, vec![1, 2, 3]);
        assert!(!has_account(&s, ClientId(4)));
    }
}
