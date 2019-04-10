use std::cell::RefCell;
use std::collections::HashMap;
use aion_types::{Address, U256, H256};
use state::{
    AionVMAccount,
    VMAccount,
    RequireCache,
    Backend,
    AccType,
};

use factory::Factories;
use trie;
use trie::{Trie, TrieError};

#[derive(Eq, PartialEq, Clone, Copy, Debug)]
/// Account modification state. Used to check if the account was
/// Modified in between commits and overall.
pub enum AccountState {
    /// Account was loaded from disk and never modified in this state object.
    CleanFresh,
    /// Account was loaded from the global cache and never modified.
    CleanCached,
    /// Account has been modified and is not committed to the trie yet.
    /// This is set if any of the account data is changed, including
    /// storage and code.
    Dirty,
    /// Account was modified and committed to the trie.
    Committed,
}

#[derive(Debug)]
/// In-memory copy of the account data. Holds the optional account
/// and the modification status.
/// Account entry can contain existing (`Some`) or non-existing
/// account (`None`)
pub struct AccountEntry<T>
where T: VMAccount
{
    /// Account entry. `None` if account known to be non-existant.
    pub account: Option<T>,
    /// Unmodified account balance.
    pub old_balance: Option<U256>,
    /// Entry state.
    pub state: AccountState,
}

// Account cache item. Contains account data and
// modification state
impl AccountEntry<AionVMAccount> {
    pub fn is_dirty(&self) -> bool { self.state == AccountState::Dirty }

    /// Clone dirty data into new `AccountEntry`. This includes
    /// basic account data and modified storage keys.
    /// Returns None if clean.
    pub fn clone_if_dirty(&self) -> Option<AccountEntry<AionVMAccount>> {
        match self.is_dirty() {
            true => Some(self.clone_dirty()),
            false => None,
        }
    }

    /// Clone dirty data into new `AccountEntry`. This includes
    /// basic account data and modified storage keys.
    pub fn clone_dirty(&self) -> AccountEntry<AionVMAccount> {
        AccountEntry {
            old_balance: self.old_balance,
            account: self.account.as_ref().map(AionVMAccount::clone_dirty),
            state: self.state,
        }
    }

    // Create a new account entry and mark it as dirty.
    pub fn new_dirty(account: Option<AionVMAccount>) -> AccountEntry<AionVMAccount> {
        AccountEntry {
            old_balance: account.as_ref().map(|a| a.balance().clone()),
            account: account,
            state: AccountState::Dirty,
        }
    }

    // Create a new account entry and mark it as clean.
    pub fn new_clean(account: Option<AionVMAccount>) -> AccountEntry<AionVMAccount> {
        AccountEntry {
            old_balance: account.as_ref().map(|a| a.balance().clone()),
            account: account,
            state: AccountState::CleanFresh,
        }
    }

    // Create a new account entry and mark it as clean and cached.
    pub fn new_clean_cached(account: Option<AionVMAccount>) -> AccountEntry<AionVMAccount> {
        AccountEntry {
            old_balance: account.as_ref().map(|a| a.balance().clone()),
            account: account,
            state: AccountState::CleanCached,
        }
    }

    pub fn overwrite_with(&mut self, other: AccountEntry<AionVMAccount>) {
        self.state = other.state;
        match other.account {
            Some(acc) => {
                if let Some(ref mut ours) = self.account {
                    ours.overwrite_with(acc);
                }
            }
            None => self.account = None,
        }
    }
}