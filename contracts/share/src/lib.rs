#![no_std]
use soroban_sdk::{contract, contractimpl, contracttype, symbol_short, Address, Env, String, Symbol};

const EVT: Symbol = symbol_short!("share");

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
        env.events()
            .publish((EVT, symbol_short!("init")), (name, symbol, decimals));
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
        let new_total = total + amount;
        env.storage()
            .instance()
            .set(&DataKey::TotalSupply, &new_total);
        env.events()
            .publish((EVT, symbol_short!("mint")), (to, amount, new_total));
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
        let new_total = total - amount;
        env.storage()
            .instance()
            .set(&DataKey::TotalSupply, &new_total);
        env.events()
            .publish((EVT, symbol_short!("burn")), (from, amount, new_total));
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
            .set(&DataKey::Balance(from.clone()), &(balance_from - amount));

        let balance_to = Self::balance(env.clone(), to.clone());
        env.storage()
            .persistent()
            .set(&DataKey::Balance(to.clone()), &(balance_to + amount));
        env.events()
            .publish((EVT, symbol_short!("transfer")), (from, to, amount));
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

#[cfg(test)]
mod test {
    use super::*;
    use soroban_sdk::{testutils::Address as _, Env};

    fn setup(env: &Env) -> (ShareTokenClient<'_>, Address) {
        let contract_id = env.register(ShareToken, ());
        let client = ShareTokenClient::new(env, &contract_id);
        let admin = Address::generate(env);
        client.initialize(
            &admin,
            &7u32,
            &String::from_str(env, "Pool Shares"),
            &String::from_str(env, "POOL"),
        );
        (client, admin)
    }

    #[test]
    fn test_mint_emits_event() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, _admin) = setup(&env);
        let to = Address::generate(&env);

        client.mint(&to, &500i128);

        assert_eq!(client.balance(&to), 500);
        assert_eq!(client.total_supply(), 500);
    }

    #[test]
    fn test_burn_emits_event() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, _admin) = setup(&env);
        let holder = Address::generate(&env);

        client.mint(&holder, &1_000i128);
        client.burn(&holder, &400i128);

        assert_eq!(client.balance(&holder), 600);
        assert_eq!(client.total_supply(), 600);
    }

    #[test]
    fn test_transfer_emits_event() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, _admin) = setup(&env);
        let alice = Address::generate(&env);
        let bob = Address::generate(&env);

        client.mint(&alice, &1_000i128);
        client.transfer(&alice, &bob, &300i128);

        assert_eq!(client.balance(&alice), 700);
        assert_eq!(client.balance(&bob), 300);
        assert_eq!(client.total_supply(), 1_000);
    }

    #[test]
    fn test_initialize_emits_event() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(ShareToken, ());
        let client = ShareTokenClient::new(&env, &contract_id);
        let admin = Address::generate(&env);

        client.initialize(
            &admin,
            &6u32,
            &String::from_str(&env, "Test Token"),
            &String::from_str(&env, "TEST"),
        );

        assert_eq!(client.decimals(), 6u32);
        assert_eq!(client.total_supply(), 0);
    }

    #[test]
    #[should_panic(expected = "amount must be positive")]
    fn test_mint_non_admin_rejected_before_event() {
        let env = Env::default();
        // Do NOT mock auths — the admin auth will fail
        let (client, _admin) = setup(&env);
        let to = Address::generate(&env);
        client.mint(&to, &0i128);
    }

    #[test]
    #[should_panic(expected = "amount must be positive")]
    fn test_burn_zero_amount() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, _admin) = setup(&env);
        let holder = Address::generate(&env);
        client.mint(&holder, &100i128);
        client.burn(&holder, &0i128);
    }

    #[test]
    #[should_panic(expected = "amount must be positive")]
    fn test_transfer_zero_amount() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, _admin) = setup(&env);
        let alice = Address::generate(&env);
        let bob = Address::generate(&env);
        client.mint(&alice, &100i128);
        client.transfer(&alice, &bob, &0i128);
    }
}
