use crate::address::Address;
use crate::account::{Account, Balance};
use std::error::Error;

pub trait StateView {
    fn get_account(&mut self, address: &Address) -> Result<Option<Account>, Box<dyn Error>>;
    fn set_account(&mut self, acc: &Account) -> Result<(), Box<dyn Error>>;
    fn get_balance(&mut self, address: &Address) -> Option<Balance>;
    fn set_balance(&mut self, address: &Address, amt: Balance) -> bool;
}