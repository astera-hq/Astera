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

    // --- Happy path ---

    #[test]
    fn test_initialize_sets_fields() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(ShareToken, ());
        let client = ShareTokenClient::new(&env, &contract_id);
        let admin = Address::generate(&env);

        client.initialize(
            &admin,
            &6u32,
            &String::from_str(&env, "Test Token"),
            &String::from_str(&env, "TST"),
        );

        assert_eq!(client.decimals(), 6u32);
        assert_eq!(client.name(), String::from_str(&env, "Test Token"));
        assert_eq!(client.symbol(), String::from_str(&env, "TST"));
        assert_eq!(client.total_supply(), 0);
    }

    #[test]
    fn test_mint_increases_balance_and_supply() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, _admin) = setup(&env);
        let to = Address::generate(&env);

        client.mint(&to, &500i128);

        assert_eq!(client.balance(&to), 500);
        assert_eq!(client.total_supply(), 500);
    }

    #[test]
    fn test_burn_decreases_balance_and_supply() {
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
    fn test_transfer_moves_balance_without_changing_supply() {
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
    fn test_balance_of_unknown_address_is_zero() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, _admin) = setup(&env);
        let stranger = Address::generate(&env);

        assert_eq!(client.balance(&stranger), 0);
    }

    #[test]
    fn test_total_supply_consistent_after_mint_burn_sequence() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, _admin) = setup(&env);
        let alice = Address::generate(&env);
        let bob = Address::generate(&env);

        client.mint(&alice, &1_000i128);
        client.mint(&bob, &500i128);
        assert_eq!(client.total_supply(), 1_500);

        client.burn(&alice, &200i128);
        assert_eq!(client.total_supply(), 1_300);

        client.transfer(&alice, &bob, &300i128);
        assert_eq!(client.total_supply(), 1_300);
        assert_eq!(client.balance(&alice), 500);
        assert_eq!(client.balance(&bob), 800);
    }

    // --- Error cases ---

    #[test]
    fn test_unauthorized_mint_rejected() {
        let env = Env::default();
        // No mock_all_auths — admin auth will not be satisfied
        let (client, _admin) = setup(&env);
        let to = Address::generate(&env);

        let result = client.try_mint(&to, &100i128);
        assert!(result.is_err());
    }

    #[test]
    fn test_unauthorized_burn_rejected() {
        let env = Env::default();
        // No mock_all_auths — admin auth will not be satisfied for burn
        let (client, _admin) = setup(&env);

        let result = client.try_burn(&Address::generate(&env), &50i128);
        assert!(result.is_err());
    }

    #[test]
    #[should_panic(expected = "insufficient balance")]
    fn test_burn_exceeds_balance_panics() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, _admin) = setup(&env);
        let holder = Address::generate(&env);

        client.mint(&holder, &100i128);
        client.burn(&holder, &101i128);
    }

    #[test]
    #[should_panic(expected = "insufficient balance")]
    fn test_transfer_exceeds_balance_panics() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, _admin) = setup(&env);
        let alice = Address::generate(&env);
        let bob = Address::generate(&env);

        client.mint(&alice, &50i128);
        client.transfer(&alice, &bob, &51i128);
    }

    #[test]
    #[should_panic(expected = "already initialized")]
    fn test_double_initialize_panics() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(ShareToken, ());
        let client = ShareTokenClient::new(&env, &contract_id);
        let admin = Address::generate(&env);

        client.initialize(
            &admin,
            &7u32,
            &String::from_str(&env, "Pool Shares"),
            &String::from_str(&env, "POOL"),
        );
        client.initialize(
            &admin,
            &7u32,
            &String::from_str(&env, "Pool Shares"),
            &String::from_str(&env, "POOL"),
        );
    }

    // --- Edge cases ---

    #[test]
    #[should_panic(expected = "amount must be positive")]
    fn test_mint_zero_amount_panics() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, _admin) = setup(&env);
        let to = Address::generate(&env);
        client.mint(&to, &0i128);
    }

    #[test]
    #[should_panic(expected = "amount must be positive")]
    fn test_burn_zero_amount_panics() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, _admin) = setup(&env);
        let holder = Address::generate(&env);
        client.mint(&holder, &100i128);
        client.burn(&holder, &0i128);
    }

    #[test]
    fn test_transfer_to_self_leaves_balance_unchanged() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, _admin) = setup(&env);
        let alice = Address::generate(&env);

        client.mint(&alice, &200i128);
        client.transfer(&alice, &alice, &100i128);

        assert_eq!(client.balance(&alice), 200);
        assert_eq!(client.total_supply(), 200);
    }
}
