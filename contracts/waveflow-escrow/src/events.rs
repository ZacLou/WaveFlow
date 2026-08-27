// On-chain event definitions for program lifecycle and payout auditing.
use soroban_sdk::{contracttype, Address, Symbol};

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProgramCreatedEvent {
    pub program_id: u64,
    pub maintainer: Address,
    pub github_repo: Symbol,
    pub reward_per_point: i128,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FundedEvent {
    pub program_id: u64,
    pub amount: i128,
    pub new_balance: i128,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ContributorRegisteredEvent {
    pub program_id: u64,
    pub github_username: Symbol,
    pub stellar_address: Address,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MergeRecordedEvent {
    pub program_id: u64,
    pub github_username: Symbol,
    pub pr_number: u64,
    pub points: u32,
    pub payout_amount: i128,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RatioUpdatedEvent {
    pub program_id: u64,
    pub reward_per_point: i128,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProgramPausedEvent {
    pub program_id: u64,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProgramResumedEvent {
    pub program_id: u64,
}
