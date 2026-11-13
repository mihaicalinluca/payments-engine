use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

use crate::error::AccountError;

#[derive(Debug, Default)]
pub struct Account {
    available: Decimal,
    held: Decimal,
    locked: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Deserialize, Serialize)]
#[serde(transparent)]
pub struct ClientId(pub u16);

impl Account {
    pub fn available(&self) -> Decimal {
        self.available
    }

    pub fn held(&self) -> Decimal {
        self.held
    }

    pub fn total(&self) -> Decimal {
        self.available + self.held
    }

    pub fn is_locked(&self) -> bool {
        self.locked
    }

    pub fn deposit(&mut self, amount: Decimal) -> Result<(), AccountError> {
        self.available = self
            .available
            .checked_add(amount)
            .ok_or(AccountError::Overflow)?;

        Ok(())
    }

    pub fn withdraw(&mut self, amount: Decimal) -> Result<(), AccountError> {
        if self.available < amount {
            return Err(AccountError::InsufficientFunds);
        }

        self.available -= amount;

        Ok(())
    }

    pub fn hold(&mut self, amount: Decimal) -> Result<(), AccountError> {
        self.available = self
            .available
            .checked_sub(amount)
            .ok_or(AccountError::Overflow)?;

        self.held = self
            .held
            .checked_add(amount)
            .ok_or(AccountError::Overflow)?;

        Ok(())
    }

    pub fn release(&mut self, amount: Decimal) -> Result<(), AccountError> {
        self.held = self
            .held
            .checked_sub(amount)
            .ok_or(AccountError::Overflow)?;

        self.available = self
            .available
            .checked_add(amount)
            .ok_or(AccountError::Overflow)?;

        Ok(())
    }

    pub fn charge_back(&mut self, amount: Decimal) -> Result<(), AccountError> {
        self.held = self
            .held
            .checked_sub(amount)
            .ok_or(AccountError::Overflow)?;

        self.locked = true;

        Ok(())
    }
}
