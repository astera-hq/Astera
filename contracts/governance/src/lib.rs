#![no_std]

use soroban_sdk::{
    contract, contractclient, contracterror, contractimpl, contracttype, symbol_short, Address,
    Env, String, Symbol, Vec,
};

const EVT: Symbol = symbol_short!("gov");
const DEFAULT_VOTING_PERIOD_SECS: u64 = 7 * 86_400;
const DEFAULT_EXECUTION_DELAY_SECS: u64 = 48 * 3_600;
const DEFAULT_QUORUM_BPS: u32 = 1_000;
const DEFAULT_PASS_BPS: u32 = 6_000;

#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub enum ProposalStatus {
    Active,
    Passed,
    Rejected,
    Executed,
    Cancelled,
}

#[contracttype]
#[derive(Clone, Debug)]
pub struct Proposal {
    pub id: u64,
    pub proposer: Address,
    pub description: String,
    pub target_contract: Address,
    pub function_name: String,
    pub calldata: String,
    pub votes_for: i128,
    pub votes_against: i128,
    pub status: ProposalStatus,
    pub created_at: u64,
    pub voting_ends_at: u64,
    pub execution_delay: u64,
}

#[contracttype]
#[derive(Clone)]
pub struct GovernanceConfig {
    pub admin: Address,
    pub share_token: Address,
    pub voting_period_secs: u64,
    pub quorum_bps: u32,
    pub pass_bps: u32,
    pub execution_delay_secs: u64,
    pub min_share_balance: i128,
}

#[contracttype]
pub enum DataKey {
    Config,
    Proposal(u64),
    ProposalCount,
    Vote(u64, Address),
    Initialized,
}

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum GovernanceError {
    NotInitialized = 1,
    ProposalNotFound = 2,
    ProposalInactive = 3,
    AlreadyVoted = 4,
    InsufficientShareBalance = 5,
    VotingPeriodActive = 6,
    TimelockActive = 7,
    QuorumNotMet = 8,
    InvalidProposalState = 9,
    Unauthorized = 10,
}

type GovernanceResult<T> = Result<T, GovernanceError>;

#[contractclient(name = "ShareTokenClient")]
pub trait ShareTokenContract {
    fn balance(env: Env, id: Address) -> i128;
    fn total_supply(env: Env) -> i128;
}

fn load_config(env: &Env) -> GovernanceResult<GovernanceConfig> {
    env.storage()
        .instance()
        .get(&DataKey::Config)
        .ok_or(GovernanceError::NotInitialized)
}

fn proposal_weight(env: &Env, share_token: &Address, voter: &Address) -> i128 {
    ShareTokenClient::new(env, share_token).balance(voter)
}

fn finalize_proposal(env: &Env, proposal: &mut Proposal) -> GovernanceResult<()> {
    if proposal.status != ProposalStatus::Active {
        return Ok(());
    }

    let config = load_config(env)?;
    let share_client = ShareTokenClient::new(env, &config.share_token);
    let total_supply = share_client.total_supply();
    if total_supply <= 0 {
        proposal.status = ProposalStatus::Rejected;
        return Ok(());
    }

    let total_votes = proposal.votes_for + proposal.votes_against;
    let quorum = (total_supply * config.quorum_bps as i128) / 10_000i128;
    if total_votes < quorum {
        proposal.status = ProposalStatus::Rejected;
        return Err(GovernanceError::QuorumNotMet);
    }

    if proposal.votes_for * 10_000i128 >= total_votes * config.pass_bps as i128 {
        proposal.status = ProposalStatus::Passed;
    } else {
        proposal.status = ProposalStatus::Rejected;
    }

    Ok(())
}

#[contract]
pub struct Governance;

#[contractimpl]
impl Governance {
    pub fn initialize(
        env: Env,
        admin: Address,
        share_token: Address,
        voting_period_secs: u64,
        quorum_bps: u32,
        pass_bps: u32,
        execution_delay_secs: u64,
        min_share_balance: i128,
    ) {
        if env.storage().instance().has(&DataKey::Initialized) {
            panic!("already initialized");
        }

        if quorum_bps == 0 || quorum_bps > 10_000 {
            panic!("invalid quorum");
        }
        if pass_bps <= 5_000 || pass_bps > 10_000 {
            panic!("invalid threshold");
        }

        let config = GovernanceConfig {
            admin: admin.clone(),
            share_token,
            voting_period_secs: if voting_period_secs == 0 {
                DEFAULT_VOTING_PERIOD_SECS
            } else {
                voting_period_secs
            },
            quorum_bps: if quorum_bps == 0 {
                DEFAULT_QUORUM_BPS
            } else {
                quorum_bps
            },
            pass_bps: if pass_bps == 0 {
                DEFAULT_PASS_BPS
            } else {
                pass_bps
            },
            execution_delay_secs: if execution_delay_secs == 0 {
                DEFAULT_EXECUTION_DELAY_SECS
            } else {
                execution_delay_secs
            },
            min_share_balance,
        };

        env.storage().instance().set(&DataKey::Config, &config);
        env.storage().instance().set(&DataKey::ProposalCount, &0u64);
        env.storage().instance().set(&DataKey::Initialized, &true);
    }

    pub fn create_proposal(
        env: Env,
        proposer: Address,
        description: String,
        target_contract: Address,
        function_name: String,
        calldata: String,
    ) -> Result<u64, GovernanceError> {
        proposer.require_auth();
        let config = load_config(&env)?;
        let balance = proposal_weight(&env, &config.share_token, &proposer);
        if balance < config.min_share_balance {
            return Err(GovernanceError::InsufficientShareBalance);
        }

        let count: u64 = env
            .storage()
            .instance()
            .get(&DataKey::ProposalCount)
            .unwrap_or(0);
        let id = count + 1;
        let now = env.ledger().timestamp();
        let proposal = Proposal {
            id,
            proposer: proposer.clone(),
            description,
            target_contract,
            function_name,
            calldata,
            votes_for: 0,
            votes_against: 0,
            status: ProposalStatus::Active,
            created_at: now,
            voting_ends_at: now.saturating_add(config.voting_period_secs),
            execution_delay: config.execution_delay_secs,
        };

        env.storage()
            .instance()
            .set(&DataKey::Proposal(id), &proposal);
        env.storage().instance().set(&DataKey::ProposalCount, &id);
        env.events()
            .publish((EVT, symbol_short!("create")), (id, proposer));
        Ok(id)
    }

    pub fn vote(
        env: Env,
        proposal_id: u64,
        voter: Address,
        in_favor: bool,
    ) -> Result<(), GovernanceError> {
        voter.require_auth();
        let config = load_config(&env)?;
        let mut proposal: Proposal = env
            .storage()
            .instance()
            .get(&DataKey::Proposal(proposal_id))
            .ok_or(GovernanceError::ProposalNotFound)?;
        if proposal.status != ProposalStatus::Active {
            return Err(GovernanceError::ProposalInactive);
        }
        if env
            .storage()
            .persistent()
            .has(&DataKey::Vote(proposal_id, voter.clone()))
        {
            return Err(GovernanceError::AlreadyVoted);
        }
        if env.ledger().timestamp() > proposal.voting_ends_at {
            let _ = finalize_proposal(&env, &mut proposal);
            env.storage()
                .instance()
                .set(&DataKey::Proposal(proposal_id), &proposal);
            return Err(GovernanceError::VotingPeriodActive);
        }

        let weight = proposal_weight(&env, &config.share_token, &voter);
        if weight <= 0 {
            return Err(GovernanceError::InsufficientShareBalance);
        }

        if in_favor {
            proposal.votes_for += weight;
        } else {
            proposal.votes_against += weight;
        }

        env.storage()
            .persistent()
            .set(&DataKey::Vote(proposal_id, voter.clone()), &true);
        env.storage()
            .instance()
            .set(&DataKey::Proposal(proposal_id), &proposal);
        env.events().publish(
            (EVT, symbol_short!("vote")),
            (proposal_id, voter.clone(), in_favor, weight),
        );

        Ok(())
    }

    pub fn execute_proposal(env: Env, proposal_id: u64) -> Result<(), GovernanceError> {
        let config = load_config(&env)?;
        let mut proposal: Proposal = env
            .storage()
            .instance()
            .get(&DataKey::Proposal(proposal_id))
            .ok_or(GovernanceError::ProposalNotFound)?;

        if proposal.status == ProposalStatus::Cancelled
            || proposal.status == ProposalStatus::Executed
        {
            return Err(GovernanceError::ProposalInactive);
        }
        if env.ledger().timestamp() < proposal.voting_ends_at {
            return Err(GovernanceError::VotingPeriodActive);
        }
        if env.ledger().timestamp()
            < proposal
                .voting_ends_at
                .saturating_add(config.execution_delay_secs)
        {
            return Err(GovernanceError::TimelockActive);
        }

        let finalization = finalize_proposal(&env, &mut proposal);
        env.storage()
            .instance()
            .set(&DataKey::Proposal(proposal_id), &proposal);
        if finalization.is_err() {
            return finalization;
        }
        if proposal.status != ProposalStatus::Passed {
            return Err(GovernanceError::InvalidProposalState);
        }

        proposal.status = ProposalStatus::Executed;
        env.storage()
            .instance()
            .set(&DataKey::Proposal(proposal_id), &proposal);
        env.events().publish(
            (EVT, symbol_short!("execute")),
            (
                proposal_id,
                proposal.target_contract,
                proposal.function_name,
                proposal.calldata,
            ),
        );
        Ok(())
    }

    pub fn cancel_proposal(
        env: Env,
        proposal_id: u64,
        caller: Address,
    ) -> Result<(), GovernanceError> {
        caller.require_auth();
        let config = load_config(&env)?;
        let mut proposal: Proposal = env
            .storage()
            .instance()
            .get(&DataKey::Proposal(proposal_id))
            .ok_or(GovernanceError::ProposalNotFound)?;

        if caller != proposal.proposer && caller != config.admin {
            return Err(GovernanceError::Unauthorized);
        }
        if proposal.status == ProposalStatus::Executed {
            return Err(GovernanceError::ProposalInactive);
        }

        proposal.status = ProposalStatus::Cancelled;
        env.storage()
            .instance()
            .set(&DataKey::Proposal(proposal_id), &proposal);
        env.events()
            .publish((EVT, symbol_short!("cancel")), (proposal_id, caller));
        Ok(())
    }

    pub fn get_proposal(env: Env, proposal_id: u64) -> Option<Proposal> {
        env.storage()
            .instance()
            .get(&DataKey::Proposal(proposal_id))
    }

    pub fn list_proposals(env: Env) -> Vec<Proposal> {
        let count: u64 = env
            .storage()
            .instance()
            .get(&DataKey::ProposalCount)
            .unwrap_or(0);
        let mut proposals = Vec::new(&env);
        for id in 1..=count {
            if let Some(mut proposal) = env
                .storage()
                .instance()
                .get::<DataKey, Proposal>(&DataKey::Proposal(id))
            {
                if proposal.status == ProposalStatus::Active
                    && env.ledger().timestamp() > proposal.voting_ends_at
                {
                    let _ = finalize_proposal(&env, &mut proposal);
                    env.storage()
                        .instance()
                        .set(&DataKey::Proposal(id), &proposal);
                }
                proposals.push_back(proposal);
            }
        }
        proposals
    }

    pub fn get_config(env: Env) -> Result<GovernanceConfig, GovernanceError> {
        load_config(&env)
    }
}
