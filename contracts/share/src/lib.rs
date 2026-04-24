#![no_std]
use soroban_sdk::{contract, contractimpl, contracttype, Address, Env, String};

#[contracttype]
pub enum DataKey {
    Admin,
    Name,
    Symbol,
    Decimals,
    Balance(Address),
    TotalSupply,
}

#[contract]
pub struct ShareToken;

#[contractimpl]
impl ShareToken {
    pub fn initialize(env: Env, admin: Address, decimals: u32, name: String, symbol: String) {
        if env.storage().instance().has(&DataKey::Admin) {
            panic!("already initialized");
        }
        env.storage().instance().set(&DataKey::Admin, &admin);
        env.storage().instance().set(&DataKey::Decimals, &decimals);
        env.storage().instance().set(&DataKey::Name, &name);
        env.storage().instance().set(&DataKey::Symbol, &symbol);
        env.storage().instance().set(&DataKey::TotalSupply, &0i128);
    }

    pub fn mint(env: Env, to: Address, amount: i128) {
        let admin: Address = env.storage().instance().get(&DataKey::Admin).unwrap();
        admin.require_auth();
        if amount <= 0 {
            panic!("amount must be positive");
        }
        let balance = Self::balance(env.clone(), to.clone());
        env.storage()
            .persistent()
            .set(&DataKey::Balance(to.clone()), &(balance + amount));

        let total: i128 = env.storage().instance().get(&DataKey::TotalSupply).unwrap();
        env.storage()
            .instance()
            .set(&DataKey::TotalSupply, &(total + amount));
    }

    pub fn burn(env: Env, from: Address, amount: i128) {
        let admin: Address = env.storage().instance().get(&DataKey::Admin).unwrap();
        admin.require_auth();
        if amount <= 0 {
            panic!("amount must be positive");
        }
        let balance = Self::balance(env.clone(), from.clone());
        if balance < amount {
            panic!("insufficient balance");
        }
        env.storage()
            .persistent()
            .set(&DataKey::Balance(from.clone()), &(balance - amount));

        let total: i128 = env.storage().instance().get(&DataKey::TotalSupply).unwrap();
        env.storage()
            .instance()
            .set(&DataKey::TotalSupply, &(total - amount));
    }

    pub fn transfer(env: Env, from: Address, to: Address, amount: i128) {
        from.require_auth();
        if amount <= 0 {
            panic!("amount must be positive");
        }
        let balance_from = Self::balance(env.clone(), from.clone());
        if balance_from < amount {
            panic!("insufficient balance");
        }
        env.storage()
            .persistent()
            .set(&DataKey::Balance(from), &(balance_from - amount));

        let balance_to = Self::balance(env.clone(), to.clone());
        env.storage()
            .persistent()
            .set(&DataKey::Balance(to), &(balance_to + amount));
    }

    pub fn balance(env: Env, id: Address) -> i128 {
        env.storage()
            .persistent()
            .get(&DataKey::Balance(id))
            .unwrap_or(0)
    }

    pub fn total_supply(env: Env) -> i128 {
        env.storage()
            .instance()
            .get(&DataKey::TotalSupply)
            .unwrap_or(0)
    }

    pub fn decimals(env: Env) -> u32 {
        env.storage().instance().get(&DataKey::Decimals).unwrap()
    }

    pub fn name(env: Env) -> String {
        env.storage().instance().get(&DataKey::Name).unwrap()
    }

    pub fn symbol(env: Env) -> String {
        env.storage().instance().get(&DataKey::Symbol).unwrap()
    }
}
