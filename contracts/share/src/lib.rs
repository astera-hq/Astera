#![no_std]
use soroban_sdk::{
    contract, contractimpl, contracttype, symbol_short, token, Address, Env, String, Symbol,
    Vec,
};

const EVT: Symbol = symbol_short!("share");

const LEDGERS_PER_DAY: u32 = 17_280;
const BALANCE_LIFETIME_THRESHOLD: u32 = LEDGERS_PER_DAY * 7;
const BALANCE_BUMP_AMOUNT: u32 = LEDGERS_PER_DAY * 30;
const MAX_DECIMALS: u32 = 18;

/// Maximum number of balance checkpoints retained per holder.
/// Once the list is full the oldest entry is dropped before a new one is
/// appended, giving a bounded rolling window (≈ 1 checkpoint / ledger-second
/// worst-case, or ~2.8 years of daily snapshots at the common 1-per-day rate).
/// Governance's `balance_at` queries target recent proposal-creation timestamps,
/// so pruning ancient history does not affect correctness in practice.
pub const MAX_CHECKPOINTS: u32 = 1_024;

#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct AllowanceRecord {
    pub amount: i128,
    pub expiration_ledger: u32,
}

#[contracttype]
pub enum DataKey {
    Admin,
    Paused,
    Name,
    Symbol,
    Decimals,
    Balance(Address),
    Allowance(Address, Address),
    AllowanceExpiration(Address, Address),
    TotalSupply,
    /// Historical (timestamp, balance) checkpoints per holder, append-only and
    /// ordered by timestamp. Lets callers (e.g. governance) read a holder's
    /// balance as of a past point in time instead of their current balance,
    /// so voting power reflects the snapshot at proposal creation rather than
    /// whatever the holder's balance happens to be when they cast their vote.
    Checkpoints(Address),
    /// Ring buffer head index for the checkpoint vec — points to the oldest
    /// entry when the buffer is full, otherwise 0.
    CheckpointsHead(Address),
}

/// Records a checkpoint of `who`'s new balance at the current ledger timestamp.
/// Multiple writes within the same timestamp overwrite the last checkpoint for
/// that timestamp rather than appending, keeping the list free of duplicates.
/// Uses a ring buffer (with a stored head index) to avoid O(n) vec.remove(0).
fn write_checkpoint(env: &Env, who: &Address, new_balance: i128) {
    let key = DataKey::Checkpoints(who.clone());
    let head_key = DataKey::CheckpointsHead(who.clone());
    let mut checkpoints: Vec<(u64, i128)> = env
        .storage()
        .persistent()
        .get(&key)
        .unwrap_or_else(|_| Vec::new(env));
    let mut head: u32 = env
        .storage()
        .persistent()
        .get(&head_key)
        .unwrap_or(0);

    let now = env.ledger().timestamp();
    if let Some(last) = checkpoints.last() {
        if last.0 == now {
            checkpoints.set(checkpoints.len() - 1, (now, new_balance));
            env.storage().persistent().set(&key, &checkpoints);
            env.storage().persistent().extend_ttl(&key, BALANCE_LIFETIME_THRESHOLD, BALANCE_BUMP_AMOUNT);
            return;
        }
    }

    // Append or overwrite at ring buffer head if full
    if checkpoints.len() < MAX_CHECKPOINTS as usize {
        checkpoints.push_back((now, new_balance));
    } else {
        checkpoints.set(head as usize, (now, new_balance));
        head = (head + 1) % MAX_CHECKPOINTS;
        env.storage().persistent().set(&head_key, &head);
    }
    env.storage().persistent().set(&key, &checkpoints);
    env.storage().persistent().extend_ttl(&key, BALANCE_LIFETIME_THRESHOLD, BALANCE_BUMP_AMOUNT);
}

fn require_not_paused(env: &Env) {
    if env
        .storage()
        .instance()
        .get(&DataKey::Paused)
        .unwrap_or(false)
    {
        panic!("contract is paused");
    }
}

#[contract]
pub struct ShareToken;

#[contractimpl]
impl ShareToken {
    pub fn initialize(env: Env, admin: Address, decimals: u32, name: String, symbol: String) {
        admin.require_auth();
        if env.storage().instance().has(&DataKey::Admin) {
            panic!("already initialized");
        }
        if decimals > MAX_DECIMALS {
            panic!("decimals must not exceed 18");
        }
        env.storage().instance().set(&DataKey::Admin, &admin);
        env.storage().instance().set(&DataKey::Paused, &false);
        env.storage().instance().set(&DataKey::Decimals, &decimals);
        env.storage().instance().set(&DataKey::Name, &name);
        env.storage().instance().set(&DataKey::Symbol, &symbol);
        env.storage().instance().set(&DataKey::TotalSupply, &0i128);
        env.events()
            .publish((EVT, symbol_short!("init")), (name, symbol, decimals));
    }

    pub fn pause(env: Env, admin: Address) {
        admin.require_auth();
        let stored_admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .unwrap_or_else(|_| {
                panic!("contract not initialized");
            });
        if admin != stored_admin {
            panic!("unauthorized");
        }
        env.storage().instance().set(&DataKey::Paused, &true);
        env.events().publish((EVT, symbol_short!("paused")), admin);
    }

    pub fn unpause(env: Env, admin: Address) {
        admin.require_auth();
        let stored_admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .unwrap_or_else(|_| {
                panic!("contract not initialized");
            });
        if admin != stored_admin {
            panic!("unauthorized");
        }
        env.storage().instance().set(&DataKey::Paused, &false);
        env.events().publish((EVT, symbol_short!("unpause")), admin);
    }

    pub fn is_paused(env: Env) -> bool {
        env.storage().instance().get(&DataKey::Paused).unwrap_or(false)
    }

    pub fn admin(env: Env) -> Address {
        env.storage()
            .instance()
            .get(&DataKey::Admin)
            .unwrap_or_else(|_| {
                panic!("contract not initialized");
            })
    }

    /// Rotates the admin to `new_admin`. Only the current admin may call this.
    pub fn set_admin(env: Env, new_admin: Address) {
        let admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .unwrap_or_else(|_| {
                panic!("contract not initialized");
            });
        admin.require_auth();
        env.storage().instance().set(&DataKey::Admin, &new_admin);
        env.events()
            .publish((EVT, symbol_short!("set_admin")), (admin, new_admin));
    }

    pub fn mint(env: Env, to: Address, amount: i128) {
        require_not_paused(&env);
        let admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .unwrap_or_else(|_| {
                panic!("contract not initialized");
            });
        admin.require_auth();
        if amount <= 0 {
            panic!("amount must be positive");
        }
        let balance = Self::balance(env.clone(), to.clone());
        let new_balance = balance + amount;
        let balance_key = DataKey::Balance(to.clone());
        env.storage()
            .persistent()
            .set(&balance_key, &new_balance);
        env.storage().persistent().extend_ttl(&balance_key, BALANCE_LIFETIME_THRESHOLD, BALANCE_BUMP_AMOUNT);
        write_checkpoint(&env, &to, new_balance);

        let total: i128 = env.storage().instance().get(&DataKey::TotalSupply).unwrap_or(0);
        let new_total = total
            .checked_add(amount)
            .expect("total supply overflow");
        env.storage()
            .instance()
            .set(&DataKey::TotalSupply, &new_total);
        env.events()
            .publish((EVT, symbol_short!("mint")), (to, amount, new_total));
    }

    pub fn burn(env: Env, from: Address, amount: i128) {
        require_not_paused(&env);
        from.require_auth();
        if amount <= 0 {
            panic!("amount must be positive");
        }
        let balance = Self::balance(env.clone(), from.clone());
        if balance < amount {
            panic!("insufficient balance");
        }
        let new_balance = balance - amount;
        let balance_key = DataKey::Balance(from.clone());
        env.storage()
            .persistent()
            .set(&balance_key, &new_balance);
        env.storage().persistent().extend_ttl(&balance_key, BALANCE_LIFETIME_THRESHOLD, BALANCE_BUMP_AMOUNT);
        write_checkpoint(&env, &from, new_balance);

        let total: i128 = env.storage().instance().get(&DataKey::TotalSupply).unwrap_or(0);
        let new_total = total
            .checked_sub(amount)
            .expect("total supply underflow");
        env.storage()
            .instance()
            .set(&DataKey::TotalSupply, &new_total);
        env.events()
            .publish((EVT, symbol_short!("burn")), (from, amount, new_total));
    }

    pub fn burn_from(env: Env, spender: Address, from: Address, amount: i128) {
        require_not_paused(&env);
        spender.require_auth();
        if amount <= 0 {
            panic!("amount must be positive");
        }
        let record = env.storage()
            .persistent()
            .get::<DataKey, AllowanceRecord>(&DataKey::Allowance(from.clone(), spender.clone()))
            .unwrap_or(AllowanceRecord {
                amount: 0,
                expiration_ledger: u32::MAX,
            });
        let allowed = if env.ledger().sequence() >= record.expiration_ledger as u64 {
            0
        } else {
            record.amount
        };
        if allowed < amount {
            panic!("allowance exceeded");
        }
        let balance = Self::balance(env.clone(), from.clone());
        if balance < amount {
            panic!("insufficient balance");
        }
        let new_balance = balance - amount;
        let balance_key = DataKey::Balance(from.clone());
        env.storage()
            .persistent()
            .set(&balance_key, &new_balance);
        env.storage().persistent().extend_ttl(&balance_key, BALANCE_LIFETIME_THRESHOLD, BALANCE_BUMP_AMOUNT);
        write_checkpoint(&env, &from, new_balance);

        let total: i128 = env.storage().instance().get(&DataKey::TotalSupply).unwrap_or(0);
        let new_total = total
            .checked_sub(amount)
            .expect("total supply underflow");
        env.storage()
            .instance()
            .set(&DataKey::TotalSupply, &new_total);
        let allowance_key = DataKey::Allowance(from.clone(), spender.clone());
        env.storage().persistent().set(
            &allowance_key,
            &(allowed - amount),
        );
        env.storage().persistent().extend_ttl(&allowance_key, BALANCE_LIFETIME_THRESHOLD, BALANCE_BUMP_AMOUNT);
        env.events()
            .publish((EVT, symbol_short!("burn_from")), (spender, from, amount, new_total));
    }

    pub fn transfer(env: Env, from: Address, to: Address, amount: i128) {
        require_not_paused(&env);
        from.require_auth();
        if amount <= 0 {
            panic!("amount must be positive");
        }
        let balance_from = Self::balance(env.clone(), from.clone());
        if balance_from < amount {
            panic!("insufficient balance");
        }
        let new_balance_from = balance_from - amount;
        let balance_from_key = DataKey::Balance(from.clone());
        env.storage()
            .persistent()
            .set(&balance_from_key, &new_balance_from);
        env.storage().persistent().extend_ttl(&balance_from_key, BALANCE_LIFETIME_THRESHOLD, BALANCE_BUMP_AMOUNT);
        write_checkpoint(&env, &from, new_balance_from);

        let balance_to = Self::balance(env.clone(), to.clone());
        let new_balance_to = balance_to + amount;
        let balance_to_key = DataKey::Balance(to.clone());
        env.storage()
            .persistent()
            .set(&balance_to_key, &new_balance_to);
        env.storage().persistent().extend_ttl(&balance_to_key, BALANCE_LIFETIME_THRESHOLD, BALANCE_BUMP_AMOUNT);
        write_checkpoint(&env, &to, new_balance_to);
        env.events()
            .publish((EVT, symbol_short!("transfer")), (from, to, amount));
    }

    pub fn approve(
        env: Env,
        owner: Address,
        spender: Address,
        amount: i128,
        expiration_ledger: u32,
    ) {
        require_not_paused(&env);
        owner.require_auth();
        if amount < 0 {
            panic!("amount must be non-negative");
        }
        let allowance_key = DataKey::Allowance(owner.clone(), spender.clone());
        let record = AllowanceRecord {
            amount,
            expiration_ledger,
        };
        env.storage()
            .persistent()
            .set(&allowance_key, &record);
        env.storage().persistent().extend_ttl(&allowance_key, BALANCE_LIFETIME_THRESHOLD, BALANCE_BUMP_AMOUNT);
        env.events()
            .publish((EVT, symbol_short!("approve")), (owner, spender, amount, expiration_ledger));
    }

    pub fn allowance(env: Env, owner: Address, spender: Address) -> i128 {
        env.storage()
            .persistent()
            .get::<DataKey, AllowanceRecord>(&DataKey::Allowance(owner, spender))
            .and_then(|record| {
                if env.ledger().sequence() >= record.expiration_ledger as u64 {
                    None
                } else {
                    Some(record.amount)
                }
            })
            .unwrap_or(0)
    }

    pub fn get_allowance_expiration(env: Env, owner: Address, spender: Address) -> u32 {
        env.storage()
            .persistent()
            .get::<DataKey, AllowanceRecord>(&DataKey::Allowance(owner, spender))
            .map(|record| record.expiration_ledger)
            .unwrap_or(0)
    }

    pub fn increase_allowance(env: Env, owner: Address, spender: Address, added_amount: i128) {
        require_not_paused(&env);
        owner.require_auth();
        if added_amount <= 0 {
            panic!("added amount must be positive");
        }
        let record = env.storage()
            .persistent()
            .get::<DataKey, AllowanceRecord>(&DataKey::Allowance(owner.clone(), spender.clone()))
            .unwrap_or(AllowanceRecord {
                amount: 0,
                expiration_ledger: u32::MAX,
            });
        let new_amount = record.amount
            .checked_add(added_amount)
            .expect("allowance overflow");
        let allowance_key = DataKey::Allowance(owner.clone(), spender.clone());
        let new_record = AllowanceRecord {
            amount: new_amount,
            expiration_ledger: record.expiration_ledger,
        };
        env.storage().persistent().set(
            &allowance_key,
            &new_record,
        );
        env.storage().persistent().extend_ttl(&allowance_key, BALANCE_LIFETIME_THRESHOLD, BALANCE_BUMP_AMOUNT);
        env.events().publish(
            (EVT, symbol_short!("incrallow")),
            (owner, spender, new_amount),
        );
    }

    pub fn decrease_allowance(env: Env, owner: Address, spender: Address, subtracted_amount: i128) {
        require_not_paused(&env);
        owner.require_auth();
        if subtracted_amount <= 0 {
            panic!("subtracted amount must be positive");
        }
        let record = env.storage()
            .persistent()
            .get::<DataKey, AllowanceRecord>(&DataKey::Allowance(owner.clone(), spender.clone()))
            .unwrap_or(AllowanceRecord {
                amount: 0,
                expiration_ledger: u32::MAX,
            });
        if record.amount < subtracted_amount {
            panic!("allowance underflow");
        }
        let new_amount = record.amount - subtracted_amount;
        let allowance_key = DataKey::Allowance(owner.clone(), spender.clone());
        let new_record = AllowanceRecord {
            amount: new_amount,
            expiration_ledger: record.expiration_ledger,
        };
        env.storage().persistent().set(
            &allowance_key,
            &new_record,
        );
        env.storage().persistent().extend_ttl(&allowance_key, BALANCE_LIFETIME_THRESHOLD, BALANCE_BUMP_AMOUNT);
        env.events().publish(
            (EVT, symbol_short!("decrallow")),
            (owner, spender, new_amount),
        );
    }

    pub fn transfer_from(env: Env, spender: Address, from: Address, to: Address, amount: i128) {
        require_not_paused(&env);
        spender.require_auth();
        if amount <= 0 {
            panic!("amount must be positive");
        }
        let record = env.storage()
            .persistent()
            .get::<DataKey, AllowanceRecord>(&DataKey::Allowance(from.clone(), spender.clone()))
            .unwrap_or(AllowanceRecord {
                amount: 0,
                expiration_ledger: u32::MAX,
            });
        let allowed = if env.ledger().sequence() >= record.expiration_ledger as u64 {
            0
        } else {
            record.amount
        };
        if allowed < amount {
            panic!("allowance exceeded");
        }
        let balance_from = Self::balance(env.clone(), from.clone());
        if balance_from < amount {
            panic!("insufficient balance");
        }

        let new_balance_from = balance_from - amount;
        let balance_from_key = DataKey::Balance(from.clone());
        env.storage()
            .persistent()
            .set(&balance_from_key, &new_balance_from);
        env.storage().persistent().extend_ttl(&balance_from_key, BALANCE_LIFETIME_THRESHOLD, BALANCE_BUMP_AMOUNT);
        write_checkpoint(&env, &from, new_balance_from);
        let balance_to = Self::balance(env.clone(), to.clone());
        let new_balance_to = balance_to + amount;
        let balance_to_key = DataKey::Balance(to.clone());
        env.storage()
            .persistent()
            .set(&balance_to_key, &new_balance_to);
        env.storage().persistent().extend_ttl(&balance_to_key, BALANCE_LIFETIME_THRESHOLD, BALANCE_BUMP_AMOUNT);
        write_checkpoint(&env, &to, new_balance_to);
        let allowance_key = DataKey::Allowance(from.clone(), spender.clone());
        env.storage().persistent().set(
            &allowance_key,
            &(allowed - amount),
        );
        env.storage().persistent().extend_ttl(&allowance_key, BALANCE_LIFETIME_THRESHOLD, BALANCE_BUMP_AMOUNT);
        env.events().publish(
            (EVT, symbol_short!("xfer_from")),
            (spender, from, to, amount),
        );
    }

    pub fn balance(env: Env, id: Address) -> i128 {
        env.storage()
            .persistent()
            .get(&DataKey::Balance(id))
            .unwrap_or(0)
    }

    /// Returns `id`'s balance as of `timestamp` (inclusive), based on recorded
    /// checkpoints. Governance uses this to weight votes by the balance a
    /// holder had at proposal creation, rather than their balance at vote
    /// time — otherwise a holder could acquire shares mid-vote (or borrow them
    /// just long enough to vote) to inflate their voting power.
    pub fn balance_at(env: Env, id: Address, timestamp: u64) -> i128 {
        let checkpoints: Vec<(u64, i128)> = env
            .storage()
            .persistent()
            .get(&DataKey::Checkpoints(id.clone()))
            .unwrap_or_else(|_| Vec::new(&env));

        if checkpoints.is_empty() {
            return 0;
        }

        let head: u32 = env
            .storage()
            .persistent()
            .get(&DataKey::CheckpointsHead(id))
            .unwrap_or(0);
        let len = checkpoints.len() as u32;
        let is_full = len >= MAX_CHECKPOINTS;

        // Binary search for the latest checkpoint at or before `timestamp`.
        // Need to account for ring buffer if full.
        let mut lo: u32 = 0;
        let mut hi: u32 = len;
        while lo < hi {
            let mid = lo + (hi - lo) / 2;
            let actual_idx = if is_full {
                ((head + mid) % MAX_CHECKPOINTS) as usize
            } else {
                mid as usize
            };
            if checkpoints.get(actual_idx).unwrap().0 <= timestamp {
                lo = mid + 1;
            } else {
                hi = mid;
            }
        }

        if lo == 0 {
            // Query timestamp is before all surviving checkpoints.
            // Return the balance at the oldest surviving checkpoint.
            let oldest_idx = if is_full { head as usize } else { 0 };
            checkpoints.get(oldest_idx).unwrap().1
        } else {
            let actual_idx = if is_full {
                ((head + lo - 1) % MAX_CHECKPOINTS) as usize
            } else {
                (lo - 1) as usize
            };
            checkpoints.get(actual_idx).unwrap().1
        }
    }

    pub fn total_supply(env: Env) -> i128 {
        env.storage()
            .instance()
            .get(&DataKey::TotalSupply)
            .unwrap_or(0)
    }

    pub fn decimals(env: Env) -> u32 {
        env.storage()
            .instance()
            .get(&DataKey::Decimals)
            .unwrap_or(0)
    }

    pub fn name(env: Env) -> String {
        env.storage()
            .instance()
            .get(&DataKey::Name)
            .unwrap_or_else(|_| String::from_utf8(vec![]).unwrap())
    }

    pub fn symbol(env: Env) -> String {
        env.storage()
            .instance()
            .get(&DataKey::Symbol)
            .unwrap_or_else(|_| String::from_utf8(vec![]).unwrap())
    }
}

#[contractimpl]
impl token::TokenInterface for ShareToken {
    fn transfer(env: Env, from: Address, to: Address, amount: i128) {
        ShareToken::transfer(env, from, to, amount);
    }

    fn transfer_from(env: Env, spender: Address, from: Address, to: Address, amount: i128) {
        ShareToken::transfer_from(env, spender, from, to, amount);
    }

    fn approve(env: Env, owner: Address, spender: Address, amount: i128, expiration_ledger: u32) {
        ShareToken::approve(env, owner, spender, amount, expiration_ledger);
    }

    fn allowance(env: Env, owner: Address, spender: Address) -> i128 {
        ShareToken::allowance(env, owner, spender)
    }

    fn balance(env: Env, id: Address) -> i128 {
        ShareToken::balance(env, id)
    }

    fn transfer_to_host(env: Env, from: Address, amount: i128) {
        require_not_paused(&env);
        from.require_auth();
        if amount <= 0 {
            panic!("amount must be positive");
        }
        let balance = ShareToken::balance(env.clone(), from.clone());
        if balance < amount {
            panic!("insufficient balance");
        }
        let new_balance = balance - amount;
        env.storage()
            .persistent()
            .set(&DataKey::Balance(from.clone()), &new_balance);
        write_checkpoint(&env, &from, new_balance);

        let total: i128 = env.storage().instance().get(&DataKey::TotalSupply).unwrap();
        let new_total = total - amount;
        env.storage()
            .instance()
            .set(&DataKey::TotalSupply, &new_total);
        env.events()
            .publish((EVT, symbol_short!("burn")), (from, amount, new_total));
    }

    fn transfer_from_host(env: Env, to: Address, amount: i128) {
        require_not_paused(&env);
        if amount <= 0 {
            panic!("amount must be positive");
        }
        let balance = ShareToken::balance(env.clone(), to.clone());
        let new_balance = balance + amount;
        env.storage()
            .persistent()
            .set(&DataKey::Balance(to.clone()), &new_balance);
        write_checkpoint(&env, &to, new_balance);

        let total: i128 = env.storage().instance().get(&DataKey::TotalSupply).unwrap();
        let new_total = total + amount;
        env.storage()
            .instance()
            .set(&DataKey::TotalSupply, &new_total);
        env.events()
            .publish((EVT, symbol_short!("mint")), (to, amount, new_total));
    }

    fn burn(env: Env, from: Address, amount: i128) {
        ShareToken::burn(env, from, amount);
    }

    fn burn_from(env: Env, spender: Address, from: Address, amount: i128) {
        ShareToken::burn_from(env, spender, from, amount);
    }

    fn decimals(env: Env) -> u32 {
        ShareToken::decimals(env)
    }

    fn name(env: Env) -> String {
        ShareToken::name(env)
    }

    fn symbol(env: Env) -> String {
        ShareToken::symbol(env)
    }

    fn total_supply(env: Env) -> i128 {
        ShareToken::total_supply(env)
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use soroban_sdk::testutils::{Address as _, Ledger};
    use soroban_sdk::Env;

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
    fn test_mint_requires_admin_auth() {
        let env = Env::default();
        // No mock_all_auths — admin auth check must be satisfied
        let (client, _admin) = setup(&env);
        let to = Address::generate(&env);
        let result = client.try_mint(&to, &100i128);
        assert!(result.is_err());
    }

    #[test]
    fn test_set_admin_rotates_admin() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, admin) = setup(&env);
        let new_admin = Address::generate(&env);

        assert_eq!(client.admin(), admin);
        client.set_admin(&new_admin);
        assert_eq!(client.admin(), new_admin);

        // The new admin can now mint, the old one cannot.
        let to = Address::generate(&env);
        client.mint(&to, &100i128);
        assert_eq!(client.balance(&to), 100);
    }

    #[test]
    fn test_set_admin_requires_current_admin_auth() {
        let env = Env::default();
        // No mock_all_auths — only the current admin may rotate.
        let (client, _admin) = setup(&env);
        let new_admin = Address::generate(&env);
        let result = client.try_set_admin(&new_admin);
        assert!(result.is_err());
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

    #[test]
    fn test_initialize_sets_name_symbol_decimals() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(ShareToken, ());
        let client = ShareTokenClient::new(&env, &contract_id);
        let admin = Address::generate(&env);

        client.initialize(
            &admin,
            &6u32,
            &String::from_str(&env, "Test Shares"),
            &String::from_str(&env, "TST"),
        );

        assert_eq!(client.name(), String::from_str(&env, "Test Shares"));
        assert_eq!(client.symbol(), String::from_str(&env, "TST"));
        assert_eq!(client.decimals(), 6u32);
        assert_eq!(client.total_supply(), 0);
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

    #[test]
    fn test_initialize_requires_admin_auth() {
        let env = Env::default();
        // No mock_all_auths — admin auth check must be satisfied
        let contract_id = env.register(ShareToken, ());
        let client = ShareTokenClient::new(&env, &contract_id);
        let admin = Address::generate(&env);

        let result = client.try_initialize(
            &admin,
            &7u32,
            &String::from_str(&env, "Pool Shares"),
            &String::from_str(&env, "POOL"),
        );
        assert!(result.is_err());
    }

    #[test]
    #[should_panic(expected = "decimals must not exceed 18")]
    fn test_initialize_rejects_invalid_decimals() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(ShareToken, ());
        let client = ShareTokenClient::new(&env, &contract_id);
        let admin = Address::generate(&env);

        client.initialize(
            &admin,
            &19u32,
            &String::from_str(&env, "Pool Shares"),
            &String::from_str(&env, "POOL"),
        );
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

    #[test]
    fn test_balance_of_unknown_address_is_zero() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, _admin) = setup(&env);

        assert_eq!(client.balance(&Address::generate(&env)), 0);
    }

    #[test]
    fn test_total_supply_consistent_after_multi_operations() {
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

    #[test]
    fn test_approve_and_transfer_from() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, _admin) = setup(&env);
        let owner = Address::generate(&env);
        let spender = Address::generate(&env);
        let recipient = Address::generate(&env);

        client.mint(&owner, &1_000i128);
        client.approve(&owner, &spender, &400i128);
        client.transfer_from(&spender, &owner, &recipient, &250i128);

        assert_eq!(client.balance(&owner), 750);
        assert_eq!(client.balance(&recipient), 250);
        assert_eq!(client.allowance(&owner, &spender), 150);
        assert_eq!(client.total_supply(), 1_000);
    }

    #[test]
    #[should_panic(expected = "allowance exceeded")]
    fn test_transfer_from_fails_exceeds_allowance() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, _admin) = setup(&env);
        let owner = Address::generate(&env);
        let spender = Address::generate(&env);
        let recipient = Address::generate(&env);

        client.mint(&owner, &1_000i128);
        client.approve(&owner, &spender, &100i128);
        client.transfer_from(&spender, &owner, &recipient, &101i128);
    }

    #[test]
    fn test_balance_at_before_any_checkpoint_is_zero() {
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().with_mut(|l| l.timestamp = 1_000);
        let (client, _admin) = setup(&env);
        let alice = Address::generate(&env);

        assert_eq!(client.balance_at(&alice, &500), 0);
    }

    #[test]
    fn test_balance_at_reflects_balance_at_past_timestamp() {
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().with_mut(|l| l.timestamp = 1_000);
        let (client, _admin) = setup(&env);
        let alice = Address::generate(&env);

        client.mint(&alice, &100i128);

        env.ledger().with_mut(|l| l.timestamp = 2_000);
        client.mint(&alice, &400i128);

        env.ledger().with_mut(|l| l.timestamp = 3_000);
        client.burn(&alice, &200i128);

        // Balance history: t=1000 -> 100, t=2000 -> 500, t=3000 -> 300
        assert_eq!(client.balance_at(&alice, &1_000), 100);
        assert_eq!(client.balance_at(&alice, &1_500), 100);
        assert_eq!(client.balance_at(&alice, &2_000), 500);
        assert_eq!(client.balance_at(&alice, &2_999), 500);
        assert_eq!(client.balance_at(&alice, &3_000), 300);
        assert_eq!(client.balance_at(&alice, &10_000), 300);
    }

    #[test]
    fn test_balance_at_not_affected_by_later_transfers() {
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().with_mut(|l| l.timestamp = 1_000);
        let (client, _admin) = setup(&env);
        let alice = Address::generate(&env);
        let bob = Address::generate(&env);

        client.mint(&alice, &1_000i128);
        let snapshot_ts = env.ledger().timestamp();

        env.ledger().with_mut(|l| l.timestamp = 2_000);
        client.transfer(&alice, &bob, &1_000i128);

        // Historical balance at proposal-creation time is unaffected by the
        // later transfer that drained alice's live balance to zero.
        assert_eq!(client.balance_at(&alice, &snapshot_ts), 1_000);
        assert_eq!(client.balance(&alice), 0);
    }

    #[test]
    fn test_balance_at_dedupes_same_timestamp_writes() {
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().with_mut(|l| l.timestamp = 1_000);
        let (client, _admin) = setup(&env);
        let alice = Address::generate(&env);

        // transfer-to-self writes two checkpoints for alice at the same
        // timestamp; balance_at must reflect the final value, not stack
        // duplicate entries.
        client.mint(&alice, &200i128);
        client.transfer(&alice, &alice, &100i128);
        assert_eq!(client.balance_at(&alice, &1_000), 200);
    }

    #[test]
    fn test_balance_at_past_max_checkpoints_evicts_oldest() {
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().with_mut(|l| l.timestamp = 1_000);
        let (client, _admin) = setup(&env);
        let alice = Address::generate(&env);

        // Mint once per second, past MAX_CHECKPOINTS, so write_checkpoint
        // starts evicting the oldest entry on each subsequent write.
        for i in 0..(MAX_CHECKPOINTS + 1) {
            env.ledger().with_mut(|l| l.timestamp = 1_000 + i as u64);
            client.mint(&alice, &1i128);
        }

        let final_balance = (MAX_CHECKPOINTS + 1) as i128;
        assert_eq!(client.balance(&alice), final_balance);

        // The very first checkpoint (ts = 1_000) was evicted to make room,
        // so a query at or before it now finds no surviving entry.
        assert_eq!(client.balance_at(&alice, &1_000), 0);

        // The oldest surviving checkpoint (ts = 1_001) still resolves.
        assert_eq!(client.balance_at(&alice, &1_001), 1);

        // Recent history is untouched by eviction.
        let last_ts = 1_000 + MAX_CHECKPOINTS as u64;
        assert_eq!(client.balance_at(&alice, &last_ts), final_balance);
    }
}
