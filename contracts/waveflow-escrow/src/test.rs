// Soroban unit tests proving fund → register → record_merge payout loop.
use crate::{WaveFlowEscrow, WaveFlowEscrowClient};
use soroban_sdk::testutils::{Address as _, Ledger};
use soroban_sdk::{token, Address, Env, Symbol};

fn setup(env: &Env) -> (Address, Address, Address, Address, Address) {
    env.mock_all_auths();

    let admin = Address::generate(env);
    let gateway = Address::generate(env);
    let maintainer = Address::generate(env);
    let contributor_wallet = Address::generate(env);

    let token_admin = Address::generate(env);
    let token_contract = env.register_stellar_asset_contract(token_admin.clone());
    let token_client = token::Client::new(env, &token_contract);
    token_client.mint(&maintainer, &10_000);

    let contract_id = env.register(WaveFlowEscrow, ());
    let client = WaveFlowEscrowClient::new(env, &contract_id);
    client.initialize(&admin, &gateway, &token_contract);

    (
        contract_id,
        gateway,
        maintainer,
        contributor_wallet,
        token_contract,
    )
}

#[test]
fn record_merge_pays_contributor() {
    let env = Env::default();
    env.ledger().with_mut(|li| li.timestamp = 1_700_000_000);

    let (contract_id, gateway, maintainer, contributor_wallet, token_contract) = setup(&env);
    let client = WaveFlowEscrowClient::new(&env, &contract_id);
    let token_client = token::Client::new(&env, &token_contract);

    let repo = Symbol::new(&env, "StellarRoute/WaveFlow");
    let program_id = client.create_program(&maintainer, &repo, &100, &None);
    client.fund(&program_id, &maintainer, &5_000);

    let username = Symbol::new(&env, "alice-dev");
    client.register_contributor(&program_id, &maintainer, &username, &contributor_wallet);

    let payout = client.record_merge(&gateway, &program_id, &username, &42, &3);
    assert_eq!(payout, 300);
    assert_eq!(token_client.balance(&contributor_wallet), 300);
    assert_eq!(client.get_escrow_balance(&program_id), 4_700);
    assert!(client.is_processed(&program_id, &42));
}

#[test]
fn duplicate_pr_is_rejected() {
    let env = Env::default();
    let (contract_id, gateway, maintainer, contributor_wallet, _) = setup(&env);
    let client = WaveFlowEscrowClient::new(&env, &contract_id);

    let repo = Symbol::new(&env, "org/repo");
    let program_id = client.create_program(&maintainer, &repo, &10, &None);
    client.fund(&program_id, &maintainer, &100);

    let username = Symbol::new(&env, "bob");
    client.register_contributor(&program_id, &maintainer, &username, &contributor_wallet);

    client.record_merge(&gateway, &program_id, &username, &7, &1);
    let err = client.try_record_merge(&gateway, &program_id, &username, &7, &1);
    assert!(err.is_err());
}

#[test]
fn paused_program_rejects_merge() {
    let env = Env::default();
    let (contract_id, gateway, maintainer, contributor_wallet, _) = setup(&env);
    let client = WaveFlowEscrowClient::new(&env, &contract_id);

    let repo = Symbol::new(&env, "org/repo");
    let program_id = client.create_program(&maintainer, &repo, &10, &None);
    client.fund(&program_id, &maintainer, &100);

    let username = Symbol::new(&env, "carol");
    client.register_contributor(&program_id, &maintainer, &username, &contributor_wallet);
    client.pause(&program_id, &maintainer);

    let err = client.try_record_merge(&gateway, &program_id, &username, &9, &1);
    assert!(err.is_err());
}

#[test]
fn milestone_cap_blocks_excess_payout() {
    let env = Env::default();
    let (contract_id, gateway, maintainer, contributor_wallet, _) = setup(&env);
    let client = WaveFlowEscrowClient::new(&env, &contract_id);

    let repo = Symbol::new(&env, "org/repo");
    let program_id = client.create_program(&maintainer, &repo, &100, &Some(150));
    client.fund(&program_id, &maintainer, &1_000);

    let username = Symbol::new(&env, "dave");
    client.register_contributor(&program_id, &maintainer, &username, &contributor_wallet);

    client.record_merge(&gateway, &program_id, &username, &1, &1);
    let err = client.try_record_merge(&gateway, &program_id, &username, &2, &1);
    assert!(err.is_err());
}

#[test]
fn invalid_repo_is_rejected() {
    let env = Env::default();
    let (contract_id, _, maintainer, _, _) = setup(&env);
    let client = WaveFlowEscrowClient::new(&env, &contract_id);

    let bad_repo = Symbol::new(&env, "invalid");
    let err = client.try_create_program(&maintainer, &bad_repo, &10, &None);
    assert!(err.is_err());
}

#[test]
fn withdraw_remaining_returns_escrow_to_maintainer() {
    let env = Env::default();
    let (contract_id, _, maintainer, contributor_wallet, token_contract) = setup(&env);
    let client = WaveFlowEscrowClient::new(&env, &contract_id);
    let token_client = token::Client::new(&env, &token_contract);

    let repo = Symbol::new(&env, "org/repo");
    let program_id = client.create_program(&maintainer, &repo, &10, &None);
    client.fund(&program_id, &maintainer, &500);
    client.pause(&program_id, &maintainer);

    let withdrawn = client.withdraw_remaining(&program_id, &maintainer);
    assert_eq!(withdrawn, 500);
    assert_eq!(token_client.balance(&maintainer), 9_500);
    let _ = contributor_wallet;
}

#[test]
fn insufficient_escrow_rejects_payout() {
    let env = Env::default();
    let (contract_id, gateway, maintainer, contributor_wallet, _) = setup(&env);
    let client = WaveFlowEscrowClient::new(&env, &contract_id);

    let repo = Symbol::new(&env, "org/repo");
    let program_id = client.create_program(&maintainer, &repo, &100, &None);
    client.fund(&program_id, &maintainer, &50);

    let username = Symbol::new(&env, "eve");
    client.register_contributor(&program_id, &maintainer, &username, &contributor_wallet);

    let err = client.try_record_merge(&gateway, &program_id, &username, &1, &1);
    assert!(err.is_err());
}

#[test]
fn set_ratio_updates_reward_per_point() {
    let env = Env::default();
    let (contract_id, _, maintainer, _, _) = setup(&env);
    let client = WaveFlowEscrowClient::new(&env, &contract_id);

    let repo = Symbol::new(&env, "org/repo");
    let program_id = client.create_program(&maintainer, &repo, &10, &None);

    client.set_ratio(&program_id, &maintainer, &50);
    let program = client.get_program(&program_id);
    assert_eq!(program.reward_per_point, 50);
}

#[test]
fn set_ratio_rejects_zero() {
    let env = Env::default();
    let (contract_id, _, maintainer, _, _) = setup(&env);
    let client = WaveFlowEscrowClient::new(&env, &contract_id);

    let repo = Symbol::new(&env, "org/repo");
    let program_id = client.create_program(&maintainer, &repo, &10, &None);

    let err = client.try_set_ratio(&program_id, &maintainer, &0);
    assert!(err.is_err());
}

#[test]
fn set_ratio_rejects_negative() {
    let env = Env::default();
    let (contract_id, _, maintainer, _, _) = setup(&env);
    let client = WaveFlowEscrowClient::new(&env, &contract_id);

    let repo = Symbol::new(&env, "org/repo");
    let program_id = client.create_program(&maintainer, &repo, &10, &None);

    let err = client.try_set_ratio(&program_id, &maintainer, &-1);
    assert!(err.is_err());
}

#[test]
fn set_ratio_rejects_non_maintainer() {
    let env = Env::default();
    let (contract_id, _, maintainer, contributor_wallet, _) = setup(&env);
    let client = WaveFlowEscrowClient::new(&env, &contract_id);

    let repo = Symbol::new(&env, "org/repo");
    let program_id = client.create_program(&maintainer, &repo, &10, &None);

    let err = client.try_set_ratio(&program_id, &contributor_wallet, &100);
    assert!(err.is_err());
}

#[test]
fn withdraw_remaining_fails_when_active() {
    let env = Env::default();
    let (contract_id, _, maintainer, _, _) = setup(&env);
    let client = WaveFlowEscrowClient::new(&env, &contract_id);

    let repo = Symbol::new(&env, "org/repo");
    let program_id = client.create_program(&maintainer, &repo, &10, &None);
    client.fund(&program_id, &maintainer, &500);

    let err = client.try_withdraw_remaining(&program_id, &maintainer);
    assert!(err.is_err());
}

#[test]
fn withdraw_remaining_rejects_non_maintainer() {
    let env = Env::default();
    let (contract_id, _, maintainer, contributor_wallet, _) = setup(&env);
    let client = WaveFlowEscrowClient::new(&env, &contract_id);

    let repo = Symbol::new(&env, "org/repo");
    let program_id = client.create_program(&maintainer, &repo, &10, &None);
    client.fund(&program_id, &maintainer, &500);
    client.pause(&program_id, &maintainer);

    let err = client.try_withdraw_remaining(&program_id, &contributor_wallet);
    assert!(err.is_err());
}
