// Core WaveFlow escrow contract implementation with program admin and payout logic.
use crate::errors::ContractError;
use crate::events::{
    ContributorRegisteredEvent, FundedEvent, MergeRecordedEvent, ProgramCreatedEvent,
    ProgramPausedEvent, ProgramResumedEvent, RatioUpdatedEvent,
};
use crate::storage::{
    is_pr_processed, mark_pr_processed, next_program_id, read_admin, read_contributor,
    read_gateway, read_program, read_token, require_active, validate_repo, write_contributor,
    write_program, DataKey,
};
use crate::types::{Contributor, Program, ProgramStatus};
use soroban_sdk::token::Client as TokenClient;
use soroban_sdk::{contract, contractimpl, symbol_short, Address, Env, Symbol};

const MIN_DEPOSIT: i128 = 1;

#[contract]
pub struct WaveFlowEscrow;

#[contractimpl]
impl WaveFlowEscrow {
    /// Bootstrap contract with admin, authorized gateway oracle, and payout token.
    pub fn initialize(env: Env, admin: Address, gateway: Address, token: Address) -> Result<(), ContractError> {
        if env.storage().instance().has(&DataKey::Admin) {
            return Err(ContractError::AlreadyInitialized);
        }
        admin.require_auth();
        env.storage().instance().set(&DataKey::Admin, &admin);
        env.storage().instance().set(&DataKey::Gateway, &gateway);
        env.storage().instance().set(&DataKey::Token, &token);
        env.storage().instance().set(&DataKey::ProgramCounter, &0u64);
        Ok(())
    }

    /// Create a bounty program linked to a GitHub repo slug.
    pub fn create_program(
        env: Env,
        maintainer: Address,
        github_repo: Symbol,
        reward_per_point: i128,
        milestone_cap: Option<i128>,
    ) -> Result<u64, ContractError> {
        maintainer.require_auth();
        if reward_per_point <= 0 {
            return Err(ContractError::InvalidRatio);
        }
        validate_repo(&github_repo)?;

        let program_id = next_program_id(&env);
        let program = Program {
            maintainer: maintainer.clone(),
            github_repo: github_repo.clone(),
            reward_per_point,
            escrow_balance: 0,
            milestone_cap,
            milestone_spent: 0,
            status: ProgramStatus::Active,
        };
        write_program(&env, program_id, &program);

        env.events().publish(
            (symbol_short!("prog_new"),),
            ProgramCreatedEvent {
                program_id,
                maintainer,
                github_repo,
                reward_per_point,
            },
        );

        Ok(program_id)
    }

    /// Deposit bounty tokens into a program escrow.
    pub fn fund(env: Env, program_id: u64, from: Address, amount: i128) -> Result<(), ContractError> {
        from.require_auth();
        if amount < MIN_DEPOSIT {
            return Err(ContractError::InsufficientDeposit);
        }

        let mut program = read_program(&env, program_id)?;
        if program.maintainer != from {
            return Err(ContractError::Unauthorized);
        }

        let token = read_token(&env)?;
        TokenClient::new(&env, &token).transfer(&from, &env.current_contract_address(), &amount);

        program.escrow_balance = program
            .escrow_balance
            .checked_add(amount)
            .ok_or(ContractError::Overflow)?;
        write_program(&env, program_id, &program);

        env.events().publish(
            (symbol_short!("funded"),),
            FundedEvent {
                program_id,
                amount,
                new_balance: program.escrow_balance,
            },
        );

        Ok(())
    }

    /// Map a GitHub username to a contributor Stellar address for payouts.
    pub fn register_contributor(
        env: Env,
        program_id: u64,
        caller: Address,
        github_username: Symbol,
        stellar_address: Address,
    ) -> Result<(), ContractError> {
        caller.require_auth();
        let program = read_program(&env, program_id)?;
        if program.maintainer != caller {
            return Err(ContractError::Unauthorized);
        }

        let key = DataKey::Contributor(program_id, github_username.clone());
        if env.storage().persistent().has(&key) {
            return Err(ContractError::ContributorAlreadyRegistered);
        }

        write_contributor(
            &env,
            program_id,
            &github_username,
            &Contributor {
                stellar_address: stellar_address.clone(),
            },
        );

        env.events().publish(
            (symbol_short!("contrib"),),
            ContributorRegisteredEvent {
                program_id,
                github_username,
                stellar_address,
            },
        );

        Ok(())
    }

    /// Authorized gateway records a merged PR and pays the contributor.
    pub fn record_merge(
        env: Env,
        gateway: Address,
        program_id: u64,
        github_username: Symbol,
        pr_number: u64,
        points: u32,
    ) -> Result<i128, ContractError> {
        gateway.require_auth();
        if read_gateway(&env)? != gateway {
            return Err(ContractError::Unauthorized);
        }
        if points == 0 {
            return Err(ContractError::InvalidRatio);
        }
        if is_pr_processed(&env, program_id, pr_number) {
            return Err(ContractError::PrAlreadyProcessed);
        }

        let mut program = read_program(&env, program_id)?;
        require_active(&program)?;

        let contributor = read_contributor(&env, program_id, &github_username)?;
        let payout = (i128::from(points)).checked_mul(program.reward_per_point).ok_or(ContractError::Overflow)?;

        if program.escrow_balance < payout {
            return Err(ContractError::InsufficientEscrow);
        }

        if let Some(cap) = program.milestone_cap {
            let new_spent = program
                .milestone_spent
                .checked_add(payout)
                .ok_or(ContractError::Overflow)?;
            if new_spent > cap {
                return Err(ContractError::MilestoneCapExceeded);
            }
            program.milestone_spent = new_spent;
        }

        let token = read_token(&env)?;
        TokenClient::new(&env, &token).transfer(
            &env.current_contract_address(),
            &contributor.stellar_address,
            &payout,
        );

        program.escrow_balance = program
            .escrow_balance
            .checked_sub(payout)
            .ok_or(ContractError::Overflow)?;
        write_program(&env, program_id, &program);
        mark_pr_processed(&env, program_id, pr_number);

        env.events().publish(
            (symbol_short!("merge"),),
            MergeRecordedEvent {
                program_id,
                github_username,
                pr_number,
                points,
                payout_amount: payout,
            },
        );

        Ok(payout)
    }

    /// Pause further merge attestations for a program.
    pub fn pause(env: Env, program_id: u64, maintainer: Address) -> Result<(), ContractError> {
        maintainer.require_auth();
        let mut program = read_program(&env, program_id)?;
        if program.maintainer != maintainer {
            return Err(ContractError::Unauthorized);
        }
        program.status = ProgramStatus::Paused;
        write_program(&env, program_id, &program);
        env.events().publish(
            (symbol_short!("paused"),),
            ProgramPausedEvent { program_id },
        );
        Ok(())
    }

    /// Resume merge attestations for a paused program.
    pub fn resume(env: Env, program_id: u64, maintainer: Address) -> Result<(), ContractError> {
        maintainer.require_auth();
        let mut program = read_program(&env, program_id)?;
        if program.maintainer != maintainer {
            return Err(ContractError::Unauthorized);
        }
        program.status = ProgramStatus::Active;
        write_program(&env, program_id, &program);
        env.events().publish(
            (symbol_short!("resumed"),),
            ProgramResumedEvent { program_id },
        );
        Ok(())
    }

    /// Update reward-per-point ratio (maintainer only, must stay positive).
    pub fn set_ratio(
        env: Env,
        program_id: u64,
        maintainer: Address,
        reward_per_point: i128,
    ) -> Result<(), ContractError> {
        maintainer.require_auth();
        if reward_per_point <= 0 {
            return Err(ContractError::InvalidRatio);
        }
        let mut program = read_program(&env, program_id)?;
        if program.maintainer != maintainer {
            return Err(ContractError::Unauthorized);
        }
        program.reward_per_point = reward_per_point;
        write_program(&env, program_id, &program);

        env.events().publish(
            (symbol_short!("ratio_upd"),),
            RatioUpdatedEvent {
                program_id,
                reward_per_point,
            },
        );

        Ok(())
    }

    /// Withdraw remaining escrow when program is paused.
    pub fn withdraw_remaining(
        env: Env,
        program_id: u64,
        maintainer: Address,
    ) -> Result<i128, ContractError> {
        maintainer.require_auth();
        let mut program = read_program(&env, program_id)?;
        if program.maintainer != maintainer {
            return Err(ContractError::Unauthorized);
        }
        if program.status != ProgramStatus::Paused {
            return Err(ContractError::ProgramNotPaused);
        }

        let amount = program.escrow_balance;
        if amount <= 0 {
            return Ok(0);
        }

        let token = read_token(&env)?;
        TokenClient::new(&env, &token).transfer(
            &env.current_contract_address(),
            &maintainer,
            &amount,
        );
        program.escrow_balance = 0;
        write_program(&env, program_id, &program);
        Ok(amount)
    }

    /// Read-only accessors for integrators and tests.
    pub fn get_program(env: Env, program_id: u64) -> Result<Program, ContractError> {
        read_program(&env, program_id)
    }

    pub fn get_escrow_balance(env: Env, program_id: u64) -> Result<i128, ContractError> {
        Ok(read_program(&env, program_id)?.escrow_balance)
    }

    pub fn is_processed(env: Env, program_id: u64, pr_number: u64) -> bool {
        is_pr_processed(&env, program_id, pr_number)
    }

    pub fn admin(env: Env) -> Result<Address, ContractError> {
        read_admin(&env)
    }
}
