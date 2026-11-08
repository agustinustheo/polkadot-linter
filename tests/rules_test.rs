use std::path::PathBuf;

fn fixture_is_test_file(path: &str) -> bool {
    path.starts_with("tests/")
        || path.starts_with("test/")
        || path.contains("/tests/")
        || path.contains("/test/")
        || path.contains("/mocks/")
        || path.contains("/mock/")
        || path.contains("/fuzzer/")
        || path.contains("integration_tests")
        || path.contains("integration-tests")
        || path.ends_with("_test.rs")
        || path.ends_with("_tests.rs")
        || path.ends_with("tests.rs")
        || path.ends_with("test.rs")
        || path.ends_with("mock_message_queue.rs")
        || path.ends_with("mock.rs")
        || path.ends_with("testing_utils.rs")
}

fn test_rules(
    config: &polkadot_linter::config::Config,
) -> Vec<Box<dyn polkadot_linter::rules::LintRule>> {
    let mut rules = polkadot_linter::rules::all_rules(config);
    rules.retain(|rule| config.rule_enabled(rule.id()));
    rules
}

/// Helper: create a FileContext and run a specific rule against fixture content.
/// The bundled defaults plus the default-off rules, so every rule is tested.
fn all_rules_config() -> polkadot_linter::config::Config {
    polkadot_linter::config::Config::load(
        &PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("config/all-rules.toml"),
    )
    .expect("config/all-rules.toml is valid")
}

fn check_fixture(filename: &str, content: &str) -> Vec<polkadot_linter::diagnostics::Diagnostic> {
    let config = all_rules_config();
    let path = PathBuf::from(filename);
    let rel_path = PathBuf::from(filename);
    let is_rust = filename.ends_with(".rs");
    let is_text = !is_rust;

    let ctx = polkadot_linter::engine::FileContext {
        classification_path: polkadot_linter::engine::classification_path(&path, &rel_path),
        path: path.clone(),
        rel_path,
        content,
        is_rust,
        is_text,
        is_test_file: fixture_is_test_file(filename),
        is_benchmark_file: filename.contains("benchmarking"),
        source_target_kinds: Vec::new(),
        ast: if is_rust {
            syn::parse_file(content).ok()
        } else {
            None
        },
    };

    let rules = test_rules(&config);
    let mut diags = Vec::new();
    for rule in &rules {
        if let Some(mut d) = rule.check(&ctx, &config) {
            diags.append(&mut d);
        }
    }
    diags
}

fn check_fixture_with_config(
    filename: &str,
    content: &str,
    config: &polkadot_linter::config::Config,
) -> Vec<polkadot_linter::diagnostics::Diagnostic> {
    let path = PathBuf::from(filename);
    let rel_path = PathBuf::from(filename);
    let is_rust = filename.ends_with(".rs");

    let ctx = polkadot_linter::engine::FileContext {
        classification_path: polkadot_linter::engine::classification_path(&path, &rel_path),
        path: path.clone(),
        rel_path,
        content,
        is_rust,
        is_text: !is_rust,
        is_test_file: fixture_is_test_file(filename),
        is_benchmark_file: filename.contains("benchmarking"),
        source_target_kinds: Vec::new(),
        ast: if is_rust {
            syn::parse_file(content).ok()
        } else {
            None
        },
    };

    let rules = test_rules(config);
    let mut diags = Vec::new();
    for rule in &rules {
        if let Some(mut d) = rule.check(&ctx, config) {
            diags.append(&mut d);
        }
    }
    diags
}

fn check_fixture_path(
    path: PathBuf,
    content: &str,
) -> Vec<polkadot_linter::diagnostics::Diagnostic> {
    let config = all_rules_config();
    let path_str = path.to_string_lossy().to_string();
    let is_rust = path_str.ends_with(".rs");
    let ctx = polkadot_linter::engine::FileContext {
        classification_path: polkadot_linter::engine::classification_path(&path, &path),
        path: path.clone(),
        rel_path: path.clone(),
        content,
        is_rust,
        is_text: !is_rust,
        is_test_file: fixture_is_test_file(&path_str),
        is_benchmark_file: path_str.contains("benchmarking"),
        source_target_kinds: Vec::new(),
        ast: if is_rust {
            syn::parse_file(content).ok()
        } else {
            None
        },
    };

    let rules = test_rules(&config);
    let mut diags = Vec::new();
    for rule in &rules {
        if let Some(mut d) = rule.check(&ctx, &config) {
            diags.append(&mut d);
        }
    }
    diags
}

fn has_rule(diags: &[polkadot_linter::diagnostics::Diagnostic], rule_id: &str) -> bool {
    diags.iter().any(|d| d.rule_id == rule_id)
}

#[test]
fn syntax_engine_does_not_register_compiler_backed_rules() {
    // Every rule enabled, so a default-off rule registered by mistake is still caught.
    let config = all_rules_config();
    let registered = polkadot_linter::rules::all_rules(&config)
        .into_iter()
        .map(|rule| rule.id().to_string())
        .collect::<Vec<_>>();

    for rule_id in polkadot_linter::rules::COMPILER_BACKED_RULE_IDS {
        assert!(
            !registered
                .iter()
                .any(|registered_id| registered_id == rule_id),
            "{rule_id} must have exactly one public implementation authority"
        );
    }
}

// ==========================================================================
// VAL001: Validation before heavy reads
// ==========================================================================
#[test]
fn val001_detects_heavy_read_before_validation() {
    let bad = include_str!("fixtures/bad_val001.rs");
    let diags = check_fixture("pallets/foo/src/lib.rs", bad);
    assert!(
        has_rule(&diags, "VAL001"),
        "VAL001 should fire on bad fixture. Got: {:?}",
        diags.iter().map(|d| &d.rule_id).collect::<Vec<_>>()
    );
}

#[test]
fn val001_allows_validation_before_heavy_read() {
    let good = include_str!("fixtures/good_val001.rs");
    let diags = check_fixture("pallets/foo/src/lib.rs", good);
    assert!(
        !has_rule(&diags, "VAL001"),
        "VAL001 should NOT fire on good fixture. Got: {:?}",
        diags
            .iter()
            .filter(|d| d.rule_id == "VAL001")
            .map(|d| &d.message)
            .collect::<Vec<_>>()
    );
}

#[test]
fn val001_skips_benchmark_files() {
    let bad = include_str!("fixtures/bad_val001.rs");
    let diags = check_fixture("pallets/foo/src/benchmarking.rs", bad);
    assert!(
        !has_rule(&diags, "VAL001"),
        "VAL001 should skip benchmark files"
    );
}

#[test]
fn val001_skips_try_runtime_storage_integrity_checks() {
    let code = r#"
#[cfg(any(feature = "try-runtime", test))]
fn do_try_state() -> Result<(), Error> {
    Proposals::<T>::get().into_iter().try_for_each(|proposal| {
        ensure!(ProposalOf::<T>::get(proposal).is_some(), "proposal must exist");
        Ok(())
    })?;
    Ok(())
}

pub fn dispatch(origin: OriginFor<T>, proposal: Hash) -> Result<(), Error> {
    let proposal = ProposalOf::<T>::get(proposal);
    ensure_signed(origin)?;
    Ok(())
}
"#;
    let diags = check_fixture("substrate/frame/foo/src/lib.rs", code);
    let val001 = diags
        .iter()
        .filter(|diagnostic| diagnostic.rule_id == "VAL001")
        .collect::<Vec<_>>();
    assert_eq!(
        val001.len(),
        1,
        "try-runtime-only integrity checks must be ignored while production validation-order findings remain visible: {val001:?}"
    );
}

#[test]
fn val001_ignores_post_operation_predicate_checks() {
    let code = r#"
pub fn migrate() -> Result<(), Error> {
    let mut iter = Contracts::<T>::iter();
    let remaining = iter.next();
    if !remaining.is_none() {
        log::warn!("migration has more work");
    }
    Ok(())
}
"#;
    let diags = check_fixture("substrate/frame/contracts/src/migration.rs", code);
    assert!(
        !has_rule(&diags, "VAL001"),
        "post-operation predicate checks are not validation-order findings: {diags:?}"
    );
}

#[test]
fn val001_ignores_storage_reads_inside_ensure_guards() {
    let code = r#"
pub fn update(commission: u32) -> Result<(), Error> {
    ensure!(
        commission <= GlobalMaxCommission::<T>::get().unwrap_or(u32::MAX),
        Error::<T>::CommissionExceedsGlobalMaximum
    );
    ensure!(commission <= 10, Error::<T>::CommissionExceedsMaximum);
    Ok(())
}
"#;
    let diags = check_fixture("substrate/frame/foo/src/lib.rs", code);
    assert!(
        !has_rule(&diags, "VAL001"),
        "storage reads inside validation guards do not precede those guards: {diags:?}"
    );
}

#[test]
fn val001_does_not_pair_consumed_reads_with_later_guards() {
    let code = r#"
pub fn dispatch(origin: OriginFor<T>, candidate: AccountId) -> Result<(), Error> {
    let existing = Members::<T>::get(&candidate);
    ensure!(existing.is_none(), Error::<T>::AlreadyMember);
    ensure_signed(origin)?;
    Ok(())
}
"#;
    let diags = check_fixture("substrate/frame/foo/src/lib.rs", code);
    assert!(
        !has_rule(&diags, "VAL001"),
        "a validation that consumes a storage result closes the validation-order window: {diags:?}"
    );
}

#[test]
fn val001_tracks_destructured_storage_bindings() {
    let code = r#"
pub fn dispatch(origin: OriginFor<T>, sub: AccountId) -> Result<(), Error> {
    let (owner, _) = SuperOf::<T>::get(&sub).ok_or(Error::<T>::NotSub)?;
    ensure!(owner == ensure_signed(origin)?, Error::<T>::NotOwned);
    Ok(())
}
"#;
    let diags = check_fixture("substrate/frame/foo/src/lib.rs", code);
    assert!(
        !has_rule(&diags, "VAL001"),
        "destructured storage values used by a guard are not independent preconditions: {diags:?}"
    );
}

#[test]
fn val001_tracks_multiline_storage_bindings() {
    let code = r#"
pub fn dispatch(origin: OriginFor<T>, username: Username) -> Result<(), Error> {
    let who = ensure_signed(origin)?;
    let account_of_username =
        UsernameInfoOf::<T>::get(&username).ok_or(Error::<T>::NoUsername)?.owner;
    ensure!(who == account_of_username, Error::<T>::InvalidUsername);
    Ok(())
}
"#;
    let diags = check_fixture("substrate/frame/identity/src/lib.rs", code);
    assert!(
        !has_rule(&diags, "VAL001"),
        "a guard consuming a multiline storage binding is not an independent precondition: {diags:?}"
    );
}

#[test]
fn val001_detects_multiline_dispatchable_signatures() {
    let code = r#"
pub(crate) fn submit(
    origin: OriginFor<T>,
    value: u32,
) -> DispatchResult {
    let _existing = Values::<T>::get(value);
    ensure_signed(origin)?;
    Ok(())
}
"#;
    let diags = check_fixture("pallets/foo/src/lib.rs", code);
    assert!(
        has_rule(&diags, "VAL001"),
        "VAL001 must continue through multiline function signatures: {diags:?}"
    );
}

#[test]
fn val001_ignores_storage_reads_that_select_a_guarded_branch() {
    let code = r#"
pub fn chill(stash: AccountId) -> Result<(), Error> {
    let min_active_bond = if Nominators::<T>::contains_key(&stash) {
        ensure!(Self::within_nominator_limit(), Error::<T>::CannotChillOther);
        Self::min_nominator_bond()
    } else {
        Zero::zero()
    };
    ensure!(ledger.active < min_active_bond, Error::<T>::CannotChillOther);
    Ok(())
}
"#;
    let diags = check_fixture("substrate/frame/staking-async/src/pallet/mod.rs", code);
    assert!(
        !has_rule(&diags, "VAL001"),
        "a storage read selecting a guarded branch is not an independent precondition: {diags:?}"
    );
}

#[test]
fn val001_ignores_comments_strings_and_get_constants() {
    let code = r#"
pub fn submit(origin: OriginFor<T>, value: u32) -> DispatchResult {
    let description = "avoid Foo::<T>::get() before validation";
    // Foo::<T>::get() is expensive, but this is only a comment.
    let maximum = T::MaxValues::get();
    ensure!(value < maximum, Error::<T>::TooLarge);
    Ok(())
}
"#;
    let diags = check_fixture("pallets/foo/src/lib.rs", code);
    assert!(
        !has_rule(&diags, "VAL001"),
        "non-code text and Get constants are not expensive storage reads: {diags:?}"
    );
}

#[test]
fn val001_ignores_guards_on_values_computed_after_the_read() {
    // Shape of asset-conversion `do_add_liquidity`: the guard reads a value
    // computed from balances fetched after the pool lookup.
    let code = r#"
pub fn add_liquidity(who: AccountId, amount1_desired: Balance, amount2_min: Balance) -> DispatchResult {
    let pool = Pools::<T>::get(&pool_id).ok_or(Error::<T>::PoolNotFound)?;
    let reserve1 = Self::get_balance(&pool_account, asset1.clone());
    let amount2_optimal = Self::quote(&amount1_desired, &reserve1)?;
    ensure!(amount2_optimal >= amount2_min, Error::<T>::AssetTwoDepositDidNotMeetMinimum);
    Ok(())
}
"#;
    let diags = check_fixture("substrate/frame/foo/src/lib.rs", code);
    assert!(
        !has_rule(&diags, "VAL001"),
        "a guard on a value computed from post-read pallet calls cannot be hoisted: {diags:?}"
    );
}

#[test]
fn val001_tracks_storage_values_inside_mutate_closures() {
    // Shape of assets `do_transfer_with_dust`: the guard checks the storage
    // value handed to a `try_mutate` closure.
    let code = r#"
pub fn transfer(id: AssetId, source: AccountId, maybe_need_admin: Option<AccountId>) -> DispatchResult {
    let mut source_account = Account::<T, I>::get(&id, &source).ok_or(Error::<T, I>::NoAccount)?;
    Asset::<T, I>::try_mutate(&id, |maybe_details| -> DispatchResult {
        let details = maybe_details.as_mut().ok_or(Error::<T, I>::Unknown)?;
        if let Some(need_admin) = maybe_need_admin {
            ensure!(need_admin == details.admin, Error::<T, I>::NoPermission);
        }
        Ok(())
    })
}
"#;
    let diags = check_fixture("substrate/frame/foo/src/lib.rs", code);
    assert!(
        !has_rule(&diags, "VAL001"),
        "values produced inside storage mutate closures are storage-derived: {diags:?}"
    );
}

#[test]
fn val001_tracks_storage_values_through_let_chains_and_match_arms() {
    let code = r#"
pub fn release(origin: OriginFor<T>, hash: Hash, maybe_owner: Option<AccountId>) -> DispatchResult {
    let who = ensure_signed(origin)?;
    let ledger = AgentLedger::<T>::get(&who).ok_or(Error::<T>::NotAgent)?;
    let delegation = Delegation::<T>::get(&who).ok_or(Error::<T>::NotDelegator)?;
    let amount = delegation.amount;
    ensure!(amount >= ledger.total, Error::<T>::NotEnoughFunds);
    match RequestStatusFor::<T>::get(hash).ok_or(Error::<T>::NotNoted)? {
        Requested { owner, .. } => {
            ensure!(maybe_owner.map_or(true, |c| c == owner), Error::<T>::NotAuthorized);
        }
        _ => {}
    }
    Ok(())
}
"#;
    let diags = check_fixture("substrate/frame/foo/src/lib.rs", code);
    assert!(
        !has_rule(&diags, "VAL001"),
        "transitively storage-derived locals and match bindings are data-dependent: {diags:?}"
    );
}

#[test]
fn val001_ignores_guards_in_deferred_closures_and_sibling_branches() {
    let code = r#"
pub fn set_team(origin: OriginFor<T>, id: AssetId) -> DispatchResult {
    let d = Asset::<T, I>::get(&id).ok_or(Error::<T, I>::Unknown)?;
    T::ForceOrigin::try_origin(origin).map(|_| ()).or_else(|origin| -> DispatchResult {
        let origin = ensure_signed(origin)?;
        Ok(())
    })?;
    Ok(())
}

pub fn update_roles(origin: OriginFor<T>, pool_id: PoolId) -> DispatchResult {
    let mut bonded_pool = match ensure_root(origin.clone()) {
        Ok(()) => BondedPool::<T>::get(pool_id).ok_or(Error::<T>::PoolNotFound)?,
        Err(BadOrigin) => {
            let who = ensure_signed(origin)?;
            Default::default()
        }
    };
    Ok(())
}
"#;
    let diags = check_fixture("substrate/frame/foo/src/lib.rs", code);
    assert!(
        !has_rule(&diags, "VAL001"),
        "guards in closures that may not run, or in branches excluding the read, are not hoistable: {diags:?}"
    );
}

#[test]
fn val001_ignores_guards_that_read_storage_or_call_pallet_code() {
    let code = r#"
pub fn nominate(origin: OriginFor<T>, pool_id: PoolId) -> DispatchResult {
    let who = ensure_signed(origin)?;
    let bonded_pool = BondedPool::<T>::get(pool_id).ok_or(Error::<T>::PoolNotFound)?;
    ensure!(!Self::api_pool_needs_delegate_migration(pool_id), Error::<T>::NotMigrated);
    ensure!(!Claimant::<T, I>::contains_key(&who), Error::<T, I>::AlreadyInducted);
    ensure!(!T::QueuePausedQuery::is_paused(&pool_id), Error::<T>::QueuePaused);
    Ok(())
}
"#;
    let diags = check_fixture("substrate/frame/foo/src/lib.rs", code);
    assert!(
        !has_rule(&diags, "VAL001"),
        "guards whose own cost is a storage read or opaque pallet/config call are not cheap: {diags:?}"
    );
}

#[test]
fn val001_ignores_reads_inside_debug_assertions_and_logs() {
    let code = r#"
pub fn do_reward_payout(member_account: &AccountId, member: &mut PoolMember<T>) -> DispatchResult {
    debug_assert_eq!(&mut PoolMembers::<T>::get(member_account).unwrap(), member);
    log!(debug, "min bond {:?}", MinCreateBond::<T>::get());
    ensure!(!member.active_points().is_zero(), Error::<T>::FullyUnbonding);
    Ok(())
}
"#;
    let diags = check_fixture("substrate/frame/foo/src/lib.rs", code);
    assert!(
        !has_rule(&diags, "VAL001"),
        "debug-only and logging reads are not part of the validation order: {diags:?}"
    );
}

#[test]
fn val001_still_reports_hoistable_argument_guards() {
    // Recall guard. Shapes of broker `do_partition` / `do_interlace` and
    // election-provider-multi-block `submit_page`: argument-only guards after
    // reads, including after an unrelated storage-dependent guard, after pure
    // arithmetic on arguments, and inside a `try_mutate` closure.
    let code = r#"
pub fn do_interlace(region_id: RegionId, maybe_check_owner: Option<AccountId>, pivot: CoreMask) -> DispatchResult {
    let status = Status::<T>::get().ok_or(Error::<T>::Uninitialized)?;
    let region = Regions::<T>::get(&region_id).ok_or(Error::<T>::UnknownRegion)?;
    if let Some(check_owner) = maybe_check_owner {
        ensure!(Some(check_owner) == region.owner, Error::<T>::NotOwner);
    }
    ensure!(!pivot.is_void(), Error::<T>::VoidPivot);
    Ok(())
}

pub fn do_partition(region_id: RegionId, pivot_offset: Timeslice) -> DispatchResult {
    let status = Status::<T>::get().ok_or(Error::<T>::Uninitialized)?;
    let pivot = region_id.begin.saturating_add(pivot_offset);
    ensure!(pivot > region_id.begin, Error::<T>::PivotTooEarly);
    Ok(())
}

pub fn submit_page(round: u32, who: &AccountId, page: PageIndex) -> DispatchResult {
    let mut metadata = SubmissionMetadataStorage::<T>::get(round, who).ok_or(Error::<T>::NotRegistered)?;
    ensure!(page < T::Pages::get(), Error::<T>::BadPageIndex);
    Ok(())
}

pub fn set_amount(id: AssetId, who: &AccountId, amount: Balance) -> DispatchResult {
    let account = Account::<T>::get(&id, who).ok_or(Error::<T>::NoAccount)?;
    Asset::<T>::try_mutate(&id, |maybe_details| -> DispatchResult {
        ensure!(!amount.is_zero(), Error::<T>::ZeroAmount);
        Ok(())
    })
}
"#;
    let diags = check_fixture("substrate/frame/foo/src/lib.rs", code);
    let lines = diags
        .iter()
        .filter(|diagnostic| diagnostic.rule_id == "VAL001")
        .map(|diagnostic| (diagnostic.line, diagnostic.end_line))
        .collect::<Vec<_>>();
    assert_eq!(
        lines,
        vec![(3, Some(8)), (13, Some(15)), (20, Some(21)), (26, Some(28)),],
        "argument-only guards after heavy reads must keep firing: {diags:?}"
    );
}

fn val001_lines(diags: &[polkadot_linter::diagnostics::Diagnostic]) -> Vec<(usize, Option<usize>)> {
    diags
        .iter()
        .filter(|diagnostic| diagnostic.rule_id == "VAL001")
        .map(|diagnostic| (diagnostic.line, diagnostic.end_line))
        .collect()
}

#[test]
fn val001_only_counts_bindings_visible_at_the_guard() {
    // A storage-derived local suppresses a guard only when it is bound before
    // the guard and is in scope there: not a later `let`, not a binding in a
    // closed inner block, not a closure parameter outside its closure.
    let code = r#"
pub fn later_binding(amount: u32, key: u32) -> DispatchResult {
    let item = Items::<T>::get(key);
    ensure!(amount > 0, Error::<T>::Zero);
    let amount = Other::<T>::get(key);
    Ok(())
}

pub fn shadowed_in_inner_block(limit: u32, key: u32) -> DispatchResult {
    {
        let limit = Other::<T>::get(key);
        consume(limit);
    }
    let item = Items::<T>::get(key);
    ensure!(limit > 0, Error::<T>::Zero);
    Ok(())
}

pub fn closure_parameter(who: Vec<u8>, id: u32) -> DispatchResult {
    let info = Info::<T>::get(id)?;
    ensure!(who.len() < 32, Error::<T>::TooLong);
    Other::<T>::mutate(id, |who| who.push(1));
    Ok(())
}
"#;
    let diags = check_fixture("substrate/frame/foo/src/lib.rs", code);
    assert_eq!(
        val001_lines(&diags),
        vec![(3, Some(4)), (14, Some(15)), (20, Some(21))],
        "bindings that are not visible at the guard must not suppress it: {diags:?}"
    );
}

#[test]
fn val001_follows_computed_values_through_aliases() {
    let code = r#"
impl<T: Config> Pallet<T> {
    fn read_limit(key: u32) -> Result<u32, Error<T>> {
        Limits::<T>::try_get(key).map_err(|_| Error::<T>::Missing)
    }

    pub fn computed_alias(key: u32) -> DispatchResult {
        let item = Items::<T>::get(key);
        let limit = Self::read_limit(key)?;
        let alias = limit;
        ensure!(alias > 0, Error::<T>::Zero);
        Ok(())
    }

    pub fn storage_alias(key: u32) -> DispatchResult {
        let item = Items::<T>::get(key);
        let Data { limit, .. } = Other::<T>::try_get(key)?;
        let (value, _) = (limit, key);
        ensure!(value > 0, Error::<T>::Zero);
        Ok(())
    }
}
"#;
    let diags = check_fixture("substrate/frame/foo/src/lib.rs", code);
    assert!(
        !has_rule(&diags, "VAL001"),
        "aliases of storage-derived or computed values are not argument-only: {diags:?}"
    );
}

#[test]
fn val001_judges_same_crate_helpers_by_their_bodies() {
    // Pure helpers are cheap, so their guards stay reportable. Helpers that
    // read storage, directly or through another helper, are not cheap, and
    // config/trait calls the linter cannot resolve stay opaque.
    let code = r#"
impl<T: Config> Pallet<T> {
    fn positive(amount: u32) -> bool { amount > 0 }
    fn max_len() -> u32 { T::MaxLen::get() }
    fn migrated(id: u32) -> bool { Migrated::<T>::contains_key(id) }
    fn needs_migration(id: u32) -> bool { !Self::migrated(id) }

    pub fn pure_helper(amount: u32, key: u32) -> DispatchResult {
        let item = Items::<T>::get(key);
        ensure!(Self::positive(amount), Error::<T>::Zero);
        Ok(())
    }

    pub fn pure_constant_helper(name: Vec<u8>, key: u32) -> DispatchResult {
        let item = Items::<T>::get(key);
        ensure!(name.len() < Self::max_len() as usize, Error::<T>::TooLong);
        Ok(())
    }

    pub fn reading_helper(id: u32) -> DispatchResult {
        let item = Items::<T>::get(id);
        ensure!(!Self::needs_migration(id), Error::<T>::NotMigrated);
        ensure!(!T::Locker::is_locked(id), Error::<T>::Locked);
        Ok(())
    }
}
"#;
    let diags = check_fixture("substrate/frame/foo/src/lib.rs", code);
    assert_eq!(
        val001_lines(&diags),
        vec![(9, Some(10)), (15, Some(16))],
        "pure same-crate helpers are cheap; storage-reading and unresolved calls are not: {diags:?}"
    );
}

#[test]
fn val001_judges_provenance_at_the_guard() {
    // A later assignment does not make a guard storage-derived, and a local
    // rebound to an argument before the guard is no longer storage-derived.
    let code = r#"
pub fn future_assignment(amount: u32, key: u32) -> DispatchResult {
    let mut limit = amount;
    let item = Items::<T>::get(key);
    ensure!(limit > 0, Error::<T>::Zero);
    limit = Other::<T>::get(key);
    consume((item, limit));
    Ok(())
}

pub fn rebinding(amount: u32, key: u32) -> DispatchResult {
    let limit = Items::<T>::get(key);
    let limit = amount;
    ensure!(limit > 0, Error::<T>::Zero);
    consume(limit);
    Ok(())
}
"#;
    let diags = check_fixture("substrate/frame/foo/src/lib.rs", code);
    assert_eq!(
        val001_lines(&diags),
        vec![(4, Some(5)), (12, Some(14))],
        "only provenance before the guard counts: {diags:?}"
    );
}

#[test]
fn val001_reassignment_consumes_the_value_it_replaces() {
    // The right-hand side of a reassignment reads the value from before it.
    let code = r#"
pub fn reassigned(amount: u32, key: u32) -> DispatchResult {
    let mut item = Items::<T>::get(key);
    item = item.checked_add(1).ok_or(Error::<T>::Overflow)?;
    ensure!(amount > 0, Error::<T>::Zero);
    consume(item);
    Ok(())
}

pub fn rebound(amount: u32, key: u32) -> DispatchResult {
    let mut item = Items::<T>::get(key);
    let updated = item.checked_add(1).ok_or(Error::<T>::Overflow)?;
    ensure!(amount > 0, Error::<T>::Zero);
    consume(updated);
    Ok(())
}
"#;
    let diags = check_fixture("substrate/frame/foo/src/lib.rs", code);
    assert!(
        val001_lines(&diags).is_empty(),
        "a reassignment consuming the read closes the validation-order window: {diags:?}"
    );
}

#[test]
fn val001_overwrite_does_not_consume_the_value_it_replaces() {
    // The left-hand side of a reassignment whose right-hand side ignores the
    // old value does not consume the read.
    let code = r#"
pub fn overwritten(amount: u32, key: u32) -> DispatchResult {
    let mut item = Items::<T>::get(key);
    item = amount;
    ensure!(item > 0, Error::<T>::Zero);
    consume(item);
    Ok(())
}
"#;
    let diags = check_fixture("substrate/frame/foo/src/lib.rs", code);
    assert!(
        !val001_lines(&diags).is_empty(),
        "an overwrite does not consume the read: {diags:?}"
    );
}

#[test]
fn val001_follows_free_helpers_to_storage() {
    let code = r#"
fn read_limit(key: u32) -> u32 { Limits::<T>::get(key) }
impl<T: Config> Pallet<T> {
    fn has_limit(key: u32) -> bool { read_limit(key) > 0 }
    pub fn dispatch(key: u32) -> DispatchResult {
        let item = Items::<T>::get(key);
        ensure!(Self::has_limit(key), Error::<T>::Zero);
        consume(item);
        Ok(())
    }
}
"#;
    let diags = check_fixture("substrate/frame/foo/src/lib.rs", code);
    assert!(
        !has_rule(&diags, "VAL001"),
        "a helper reading storage through a free function is not cheap: {diags:?}"
    );
}

// ==========================================================================
// SEM002: Prefer collect turbofish
// ==========================================================================
#[test]
fn sem002_detects_typed_collect() {
    let bad = include_str!("fixtures/bad_sem002.rs");
    let diags = check_fixture("src/lib.rs", bad);
    assert!(
        has_rule(&diags, "SEM002"),
        "SEM002 should fire on bad fixture"
    );
}

#[test]
fn sem002_allows_turbofish_collect() {
    let good = include_str!("fixtures/good_sem002.rs");
    let diags = check_fixture("src/lib.rs", good);
    assert!(
        !has_rule(&diags, "SEM002"),
        "SEM002 should NOT fire on good fixture"
    );
}

#[test]
fn sem002_detects_typed_collect_inside_closure() {
    let code = r#"
fn build_callback() {
    let callback = || {
        let values: Vec<u8> = source.iter().copied().collect();
        values.len()
    };
    callback();
}
"#;
    let diags = check_fixture("src/lib.rs", code);
    assert!(
        has_rule(&diags, "SEM002"),
        "SEM002 should recurse into closure local bindings: {diags:?}"
    );
}

// ==========================================================================
// SEM003: Prefer reference iteration
// ==========================================================================
#[test]
fn sem003_detects_iter_pattern() {
    let bad = include_str!("fixtures/bad_sem003.rs");
    let diags = check_fixture("src/lib.rs", bad);
    assert!(
        has_rule(&diags, "SEM003"),
        "SEM003 should fire on bad fixture"
    );
}

#[test]
fn sem003_allows_ref_iteration() {
    let good = include_str!("fixtures/good_sem003.rs");
    let diags = check_fixture("src/lib.rs", good);
    assert!(
        !has_rule(&diags, "SEM003"),
        "SEM003 should NOT fire on good fixture"
    );
}

#[test]
fn sem003_skips_send_tx_utilities() {
    let bad = include_str!("fixtures/bad_sem003.rs");
    let diags = check_fixture("tools/send-tx/src/main.rs", bad);
    assert!(
        !has_rule(&diags, "SEM003"),
        "SEM003 should skip send-tx utilities that iterate over library types without IntoIterator"
    );
}

// ==========================================================================
// SEM004: No wildcard imports
// ==========================================================================
#[test]
fn sem004_detects_wildcard_import() {
    let bad = include_str!("fixtures/bad_sem004.rs");
    let diags = check_fixture("src/lib.rs", bad);
    assert!(
        has_rule(&diags, "SEM004"),
        "SEM004 should fire on bad fixture"
    );
}

#[test]
fn sem004_allows_test_wildcards() {
    let good = include_str!("fixtures/good_sem004.rs");
    let diags = check_fixture("src/lib.rs", good);
    assert!(
        !has_rule(&diags, "SEM004"),
        "SEM004 should NOT fire on good fixture"
    );
}

#[test]
fn sem004_skips_benchmark_files() {
    let bad = include_str!("fixtures/bad_sem004.rs");
    let diags = check_fixture("pallets/foo/src/benchmarking.rs", bad);
    assert!(
        !has_rule(&diags, "SEM004"),
        "SEM004 should skip benchmark files"
    );
}

#[test]
fn sem004_allows_public_reexports() {
    let good = r#"
pub use pallet::*;
pub use types::*;
"#;
    let diags = check_fixture("pallets/foo/src/lib.rs", good);
    assert!(
        !has_rule(&diags, "SEM004"),
        "SEM004 should NOT fire on public wildcard re-exports"
    );
}

#[test]
fn sem004_allows_nested_prelude_globs() {
    let good = r#"
use frame_system::pallet_prelude::{BlockNumberFor, *};
use xcm::latest::prelude::{Junction::*, Location, NetworkId};
"#;
    let diags = check_fixture("pallets/foo/src/lib.rs", good);
    assert!(
        !has_rule(&diags, "SEM004"),
        "SEM004 should NOT fire on standard prelude wildcard imports"
    );
}

#[test]
fn sem004_allows_documented_crate_globs_at_any_module_depth() {
    let good = r#"
use crate::*;
use crate::{types::BalanceOf, *};

pub mod migrations {
    use crate::*;

    pub mod v1 {
        use crate::*;
    }
}
"#;
    let diags = check_fixture("pallets/foo/src/lib.rs", good);
    assert!(
        !has_rule(&diags, "SEM004"),
        "SEM004 must honor the documented `use crate::*;` exemption: {diags:?}"
    );
}

#[test]
fn sem004_still_reports_crate_submodule_globs() {
    let bad = r#"
use crate::types::*;

pub mod nested {
    use crate::{helpers, types::*};
}
"#;
    let diags = check_fixture("pallets/foo/src/lib.rs", bad);
    let lines = diags
        .iter()
        .filter(|diagnostic| diagnostic.rule_id == "SEM004")
        .map(|diagnostic| diagnostic.line)
        .collect::<Vec<_>>();
    assert_eq!(
        lines,
        vec![2, 5],
        "only the whole-crate glob is exempt, not globs of crate submodules: {diags:?}"
    );
}

// ==========================================================================
// SEM005: Parameterised weight functions
// ==========================================================================
#[test]
fn sem005_detects_weight_multiplication() {
    let bad = include_str!("fixtures/bad_sem005.rs");
    let diags = check_fixture("src/lib.rs", bad);
    assert!(
        has_rule(&diags, "SEM005"),
        "SEM005 should fire on bad fixture"
    );
}

#[test]
fn sem005_allows_parameterised_weight() {
    let good = include_str!("fixtures/good_sem005.rs");
    let diags = check_fixture("src/lib.rs", good);
    assert!(
        !has_rule(&diags, "SEM005"),
        "SEM005 should NOT fire on good fixture"
    );
}

#[test]
fn sem005_detects_weight_multiplication_outside_weight_attr() {
    let bad = r#"
pub fn replay_missing_roots_worst_case_weight<T: Config>(chunks: u32) -> Weight {
    T::WeightInfo::send_replay_request().saturating_mul(chunks.into())
}
"#;
    let diags = check_fixture("pallets/foo/src/lib.rs", bad);
    assert!(
        has_rule(&diags, "SEM005"),
        "SEM005 should fire when a zero-arg WeightInfo call is multiplied in normal code"
    );
}

// ==========================================================================
// TST001: Prefer assert_noop
// ==========================================================================
#[test]
fn tst001_detects_manual_error_checking() {
    let bad = include_str!("fixtures/bad_tst001.rs");
    let diags = check_fixture("tests/test.rs", bad);
    assert!(
        has_rule(&diags, "TST001"),
        "TST001 should fire on bad fixture"
    );
}

#[test]
fn tst001_allows_assert_noop() {
    let good = include_str!("fixtures/good_tst001.rs");
    let diags = check_fixture("tests/test.rs", good);
    assert!(
        !has_rule(&diags, "TST001"),
        "TST001 should NOT fire on good fixture"
    );
}

#[test]
fn tst001_detects_unwrap_err_inside_assert_macro() {
    let bad = r#"
#[test]
fn manual_error_assertion() {
    let result = call();
    assert!(result.is_err(), "should fail");
    assert_eq!(result.unwrap_err(), Error::<Test>::Boom.into());
}

"#;
    let diags = check_fixture("tests/test.rs", bad);
    assert!(
        has_rule(&diags, "TST001"),
        "TST001 should fire when unwrap_err is used inside another assertion macro"
    );
}

#[test]
fn tst001_reports_one_diagnostic_for_one_manual_error_check() {
    let code = r#"
#[test]
fn manual_error_assertion() {
    let result = call();
    assert!(result.is_err());
    let _error = result.unwrap_err();
}
"#;
    let diags = check_fixture("tests/test.rs", code);
    assert_eq!(
        diags.iter().filter(|diag| diag.rule_id == "TST001").count(),
        1,
        "AST analysis must be the single authority for TST001: {diags:?}"
    );
}

// ==========================================================================
// TST002: apply_extrinsic assert_ok
// ==========================================================================
#[test]
fn tst002_detects_assert_ok_apply_extrinsic() {
    let bad = include_str!("fixtures/bad_tst002.rs");
    let diags = check_fixture("tests/test.rs", bad);
    assert!(
        has_rule(&diags, "TST002"),
        "TST002 should fire on bad fixture"
    );
}

#[test]
fn tst002_allows_proper_nested_check() {
    let good = include_str!("fixtures/good_tst002.rs");
    let diags = check_fixture("tests/test.rs", good);
    assert!(
        !has_rule(&diags, "TST002"),
        "TST002 should NOT fire on good fixture"
    );
}

#[test]
fn tst002_detects_apply_extrinsic_paths_and_methods() {
    let bad = r#"
#[test]
fn apply_extrinsic_helpers() {
    new_test_ext().execute_with(|| {
        assert_ok!(Executive::apply_extrinsic(xt.clone()));
        assert_ok!(apply_extrinsic(xt.clone()));
        assert_ok!(block_builder.apply_extrinsic(xt));
    });
}
"#;
    let diags = check_fixture("tests/test.rs", bad);
    let count = diags
        .iter()
        .filter(|diagnostic| diagnostic.rule_id == "TST002")
        .count();
    assert_eq!(
        count, 3,
        "TST002 must flag apply_extrinsic called as a path or a method: {diags:?}"
    );
}

#[test]
fn tst002_ignores_identifiers_that_only_contain_apply_extrinsic() {
    let good = r#"
#[test]
fn apply_extrinsics_returns_single_result() {
    new_test_ext().execute_with(|| {
        assert_ok!(Executive::apply_extrinsics(Block::default().extrinsics(), 1, ()), ());
        assert_ok!(apply_extrinsic_with_weight(xt), ());
        assert_ok!(Pallet::submit(origin), apply_extrinsic);
        assert_ok!(Executive::apply_extrinsic(xt.clone()).unwrap());
        assert_ok!(Executive::apply_extrinsic(xt).expect("valid extrinsic"));
        assert_ok!(Executive::apply_extrinsics(mode, xts, |xt, _| Executive::apply_extrinsic(xt)), ());
        // Single-argument forms, so the exact-name match is tested on its own.
        assert_ok!(Executive::apply_extrinsics(Block::default().extrinsics(), 1));
        assert_ok!(apply_extrinsic_with_weight(xt));
        assert_ok!(Executive::apply_extrinsics(mode, xts, |xt, _| Executive::apply_extrinsic(xt)));
        assert_ok!(Executive::apply_extrinsic(xt)?);
        // `assert_ok!(x, y)` is `assert_eq!(x, Ok(y))`, which checks both layers.
        assert_ok!(Executive::apply_extrinsic(xt), Ok(()));
    });
}
"#;
    let diags = check_fixture("tests/test.rs", good);
    assert!(
        !has_rule(&diags, "TST002"),
        "TST002 must match the apply_extrinsic identifier exactly, and accept results whose outer layer is unwrapped: {diags:?}"
    );
}

// ==========================================================================
// TST003: Imports inside closures
// ==========================================================================
#[test]
fn tst003_detects_imports_inside_closures() {
    let bad = include_str!("fixtures/bad_tst003.rs");
    let diags = check_fixture("tests/test.rs", bad);
    assert!(
        has_rule(&diags, "TST003"),
        "TST003 should fire on bad fixture"
    );
}

#[test]
fn tst003_allows_module_level_imports() {
    let good = include_str!("fixtures/good_tst003.rs");
    let diags = check_fixture("tests/test.rs", good);
    assert!(
        !has_rule(&diags, "TST003"),
        "TST003 should NOT fire on good fixture"
    );
}

#[test]
fn tst003_ignores_top_level_test_function_imports() {
    let code = r#"
#[test]
fn helper_heavy_test() {
    use crate::mock::Test;
    let _ = core::mem::size_of::<Test>();
}
"#;
    let diags = check_fixture("tests/test.rs", code);
    assert!(
        !has_rule(&diags, "TST003"),
        "TST003 should ignore imports at the top level of a test function body"
    );
}

// ==========================================================================
// TST004: Pays::Yes error path
// ==========================================================================
#[test]
fn tst004_detects_pays_no_without_companion_test() {
    let bad = include_str!("fixtures/bad_tst004.rs");
    let diags = check_fixture("pallets/foo/src/lib.rs", bad);
    assert!(
        has_rule(&diags, "TST004"),
        "TST004 should fire when Pays::No has no companion test. Got: {:?}",
        diags.iter().map(|d| &d.rule_id).collect::<Vec<_>>()
    );
}

#[test]
fn tst004_works_with_inline_cfg_test() {
    // A lib.rs that has Pays::No AND an inline #[cfg(test)] module should still be checked
    let code = r#"
pub fn do_something() -> DispatchResultWithPostInfo {
    Ok(Pays::No.into())
}

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        assert_eq!(1, 1);
    }
}
"#;
    let diags = check_fixture("pallets/foo/src/lib.rs", code);
    assert!(
        has_rule(&diags, "TST004"),
        "TST004 should fire even with inline #[cfg(test)] module"
    );
}

// ==========================================================================
// TST005: Implementation detail assertions
// ==========================================================================
#[test]
fn tst005_detects_internal_field_assertions() {
    let bad = include_str!("fixtures/bad_tst005.rs");
    let diags = check_fixture("tests/test.rs", bad);
    assert!(
        has_rule(&diags, "TST005"),
        "TST005 should fire on bad fixture"
    );
}

#[test]
fn tst005_allows_observable_assertions() {
    let good = include_str!("fixtures/good_tst005.rs");
    let diags = check_fixture("tests/test.rs", good);
    assert!(
        !has_rule(&diags, "TST005"),
        "TST005 should NOT fire on good fixture"
    );
}

// ==========================================================================
// BEN002: Benchmark verification
// ==========================================================================
#[test]
fn ben002_detects_missing_verify() {
    let bad = include_str!("fixtures/bad_ben002.rs");
    let diags = check_fixture("pallets/foo/src/benchmarking.rs", bad);
    assert!(
        has_rule(&diags, "BEN002"),
        "BEN002 should fire on bad fixture"
    );
}

#[test]
fn ben002_allows_benchmark_with_verify() {
    let good = include_str!("fixtures/good_ben002.rs");
    let diags = check_fixture("pallets/foo/src/benchmarking.rs", good);
    assert!(
        !has_rule(&diags, "BEN002"),
        "BEN002 should NOT fire on good fixture"
    );
}

#[test]
fn ben002_does_not_treat_string_literals_as_assertions() {
    let code = r#"
#[benchmark]
fn noop() {
    let _note = "assert_eq!(1, 1)";
}
"#;
    let diags = check_fixture("pallets/foo/src/benchmarking.rs", code);
    assert!(
		has_rule(&diags, "BEN002"),
		"BEN002 should still fire when benchmark only mentions assertion macros inside a string literal"
	);
}

#[test]
fn ben002_does_not_treat_block_comments_as_verification() {
    let code = r#"
#[benchmark]
fn noop() {
    /*
    assert_eq!(1, 1);
    */
}
"#;
    let diags = check_fixture("pallets/foo/src/benchmarking.rs", code);
    assert!(
        has_rule(&diags, "BEN002"),
        "BEN002 should not accept assertions inside block comments: {diags:?}"
    );
}

#[test]
fn ben002_allows_ensure_postconditions_outside_measured_block() {
    let code = r#"
#[benchmark]
fn rename_sub() -> Result<(), BenchmarkError> {
    #[extrinsic_call]
    _(RawOrigin::Signed(caller), value);

    ensure!(SomeMap::<T>::contains_key(value), "value not written");
    Ok(())
}
"#;
    let diags = check_fixture("pallets/foo/src/benchmarking.rs", code);
    assert!(
        !has_rule(&diags, "BEN002"),
        "BEN002 should accept ensure! postconditions outside the measured block"
    );
}

#[test]
fn ben002_counts_assertions_inside_measured_block_as_verification() {
    // pov-style benchmarks assert inside the measured block.
    let code = r#"
#[benchmark]
fn authorize_only() -> Result<(), BenchmarkError> {
    #[block]
    {
        assert_ok!(Pallet::<T>::authorize_call());
    }

    Ok(())
}
// `?` checks the measured call's result.
#[benchmark]
fn result_checked() -> Result<(), BenchmarkError> {
    #[block]
    {
        Pallet::<T>::authorize_call()?;
    }
    Ok(())
}
#[benchmark]
fn checked_after() -> Result<(), BenchmarkError> {
    #[block]
    {
        let _ = Pallet::<T>::authorize_call();
        assert_eq!(Value::<T>::get(), 1);
    }
    Ok(())
}
#[benchmark]
fn looped(i: Linear<0, 10>) {
    #[block]
    {
        for _ in 0..i {
            assert!(super::Pallet::<T>::recover(i).is_some());
        }
    }
}
// pov reads: every statement asserts what it measures.
#[benchmark]
fn read_only(n: Linear<0, 100>) {
    #[block]
    {
        (0..n).for_each(|_| assert_eq!(Map::<T>::get(0), Some(0)));
    }
}
#[benchmark]
fn read_then_other() {
    #[block]
    {
        assert!(Value::<T>::get().is_none());
        T::Currency::burn(1);
    }
}
// A loop counts as assertion-only only if its own body is.
#[benchmark]
fn loop_hides_other(n: Linear<0, 100>) {
    #[block]
    {
        for _ in 0..n {
            assert!(Value::<T>::get().is_none());
            T::Currency::burn(1);
        }
    }
}
// An assertion before the measured call is a precondition.
#[benchmark]
fn precondition_only() -> Result<(), BenchmarkError> {
    #[block]
    {
        assert_eq!(Value::<T>::get(), 0);
        let _ = Pallet::<T>::missing();
    }
    Ok(())
}
"#;
    let diags = check_fixture("pallets/foo/src/benchmarking.rs", code);
    let mut names = diags
        .iter()
        .filter(|d| d.rule_id == "BEN002")
        .map(|d| d.message.split('`').nth(1).unwrap().to_string())
        .collect::<Vec<_>>();
    names.sort();
    assert_eq!(
        names,
        vec!["loop_hides_other", "precondition_only", "read_then_other",],
        "{diags:?}"
    );
}

#[test]
fn ben002_skips_empty_block_override_stubs() {
    let code = r#"
#[benchmark]
fn receive_teleported_asset() -> Result<(), BenchmarkError> {
    #[block]
    {}
    Err(BenchmarkError::Override(BenchmarkResult::from_weight(Weight::MAX)))
}
#[benchmark]
fn empty_ok() -> Result<(), BenchmarkError> {
    #[block]
    {}
    Ok(())
}
#[benchmark]
fn measured_then_override() -> Result<(), BenchmarkError> {
    #[block]
    { Pallet::<T>::work(); }
    Err(BenchmarkError::Override(BenchmarkResult::from_weight(Weight::MAX)))
}
"#;
    let diags = check_fixture("pallets/foo/src/benchmarking.rs", code);
    let mut names = diags
        .iter()
        .filter(|d| d.rule_id == "BEN002")
        .map(|d| d.message.split('`').nth(1).unwrap().to_string())
        .collect::<Vec<_>>();
    names.sort();
    assert_eq!(names, vec!["empty_ok", "measured_then_override"]);
}

#[test]
fn ben002_handles_v1_benchmarks_with_individual_verify_blocks() {
    let code = r#"
frame_benchmarking::benchmarks! {
    verified {
        let value = 1u32;
    }: _(value)
    verify {
        assert_eq!(value, 1);
    }

    missing_check {
        let value = 2u32;
    }: _(value)
}
"#;
    let diags = check_fixture("pallets/foo/src/benchmarking.rs", code);
    let ben002 = diags
        .iter()
        .filter(|diagnostic| diagnostic.rule_id == "BEN002")
        .collect::<Vec<_>>();
    assert_eq!(
        ben002.len(),
        1,
        "BEN002 must report only the unverified v1 benchmark: {ben002:?}"
    );
    assert!(ben002[0].message.contains("missing_check"));
}

#[test]
fn ben002_recognizes_parameterized_benchmark_attributes() {
    let code = r#"
#[benchmark(skip_meta)]
fn no_check() {
    let value = 1u32;
    let _ = value;
}
"#;
    let diags = check_fixture("pallets/foo/src/benchmarking.rs", code);
    assert!(
        has_rule(&diags, "BEN002"),
        "BEN002 must inspect #[benchmark(...)] functions: {diags:?}"
    );
}

#[test]
fn ben001_detects_weight_function_without_matching_benchmark() {
    let root = std::env::temp_dir().join(format!(
        "polkadot-linter-ben001-missing-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let pallet_dir = root.join("pallets/foo/src");
    std::fs::create_dir_all(&pallet_dir).unwrap();

    let weights_path = pallet_dir.join("weights.rs");
    let bench_path = pallet_dir.join("benchmarking.rs");
    let weights = r#"
pub trait WeightInfo {
    fn submit() -> Weight;
    fn prune() -> Weight;
}
"#;
    let benches = r#"
#[benchmark]
fn submit() {}
"#;

    std::fs::write(&weights_path, weights).unwrap();
    std::fs::write(&bench_path, benches).unwrap();

    let diags = check_fixture_path(weights_path, weights);
    assert!(
        has_rule(&diags, "BEN001"),
        "BEN001 should fire when a weight function is missing a benchmark"
    );
}

#[test]
fn ben001_allows_matching_weight_function_benchmarks() {
    let root = std::env::temp_dir().join(format!(
        "polkadot-linter-ben001-match-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let pallet_dir = root.join("pallets/foo/src");
    std::fs::create_dir_all(&pallet_dir).unwrap();

    let weights_path = pallet_dir.join("weights.rs");
    let bench_path = pallet_dir.join("benchmarking.rs");
    let weights = r#"
pub trait WeightInfo {
    fn submit() -> Weight;
    fn prune() -> Weight;
}
"#;
    let benches = r#"
#[benchmark]
fn submit() {}

#[benchmark]
fn prune() {}
"#;

    std::fs::write(&weights_path, weights).unwrap();
    std::fs::write(&bench_path, benches).unwrap();

    let diags = check_fixture_path(weights_path, weights);
    assert!(
        !has_rule(&diags, "BEN001"),
        "BEN001 should not fire when each weight function has a benchmark"
    );
}

// ==========================================================================
// TRM001: Spelling conventions
// ==========================================================================
#[test]
fn trm001_detects_non_standard_spelling() {
    let mut config = polkadot_linter::config::Config::default();
    config
        .terminology
        .british_english
        .insert("optimisation".to_string(), "optimization".to_string());

    let bad = include_str!("fixtures/bad_trm001.rs");
    let diags = check_fixture_with_config("src/lib.rs", bad, &config);
    assert!(
        has_rule(&diags, "TRM001"),
        "TRM001 should fire on bad fixture with configured dictionary"
    );
}

#[test]
fn trm001_allows_standard_spelling() {
    let mut config = polkadot_linter::config::Config::default();
    config
        .terminology
        .british_english
        .insert("optimisation".to_string(), "optimization".to_string());

    let good = include_str!("fixtures/good_trm001.rs");
    let diags = check_fixture_with_config("src/lib.rs", good, &config);
    assert!(
        !has_rule(&diags, "TRM001"),
        "TRM001 should NOT fire on good fixture"
    );
}

#[test]
fn trm001_handles_non_ascii_before_inline_comments() {
    let mut config = polkadot_linter::config::Config::default();
    config
        .terminology
        .british_english
        .insert("optimisation".to_string(), "optimization".to_string());

    let diags = check_fixture_with_config(
        "src/lib.rs",
        "let label = \"éééé\"; // optimisation\n",
        &config,
    );
    assert!(
        has_rule(&diags, "TRM001"),
        "TRM001 must not panic or lose inline comments after non-ASCII source text"
    );
}

#[test]
fn trm001_checks_identifiers_on_lines_with_strings_and_comments() {
    let mut config = polkadot_linter::config::Config::default();
    config.terminology.check_identifiers = true;
    config
        .terminology
        .british_english
        .insert("optimisation".to_string(), "optimization".to_string());

    let diags = check_fixture_with_config(
        "src/lib.rs",
        "let optimisation = \"already optimized\"; // keep this value\n",
        &config,
    );
    assert!(
        has_rule(&diags, "TRM001"),
        "TRM001 must inspect identifiers even when the line contains strings or comments: {diags:?}"
    );
}

// ==========================================================================
// SEM006: DbWeight missing proof size
// ==========================================================================

// ==========================================================================
// SEM007: RuntimeDebug deprecated
// ==========================================================================
#[test]
fn sem007_detects_runtime_debug() {
    let bad = include_str!("fixtures/bad_sem007.rs");
    let diags = check_fixture("pallets/foo/src/types.rs", bad);
    assert!(
        has_rule(&diags, "SEM007"),
        "SEM007 should fire on RuntimeDebug usage"
    );
}

#[test]
fn sem007_allows_debug() {
    let good = include_str!("fixtures/good_sem007.rs");
    let diags = check_fixture("pallets/foo/src/types.rs", good);
    assert!(
        !has_rule(&diags, "SEM007"),
        "SEM007 should NOT fire on Debug usage"
    );
}

// ==========================================================================
// SEM008: sp_std deprecated
// ==========================================================================
#[test]
fn sem008_detects_sp_std() {
    let bad = include_str!("fixtures/bad_sem008.rs");
    let diags = check_fixture("pallets/foo/src/lib.rs", bad);
    assert!(
        has_rule(&diags, "SEM008"),
        "SEM008 should fire on sp_std usage"
    );
}

#[test]
fn sem008_allows_alloc() {
    let good = include_str!("fixtures/good_sem008.rs");
    let diags = check_fixture("pallets/foo/src/lib.rs", good);
    assert!(
        !has_rule(&diags, "SEM008"),
        "SEM008 should NOT fire on alloc usage"
    );
}

#[test]
fn sem008_detects_grouped_sp_std_usage() {
    let bad = r#"
use sp_std::{vec, vec::Vec};

fn build() -> Vec<u32> {
    sp_std::vec![1, 2, 3]
}
"#;
    let diags = check_fixture("pallets/foo/src/lib.rs", bad);
    assert!(
        has_rule(&diags, "SEM008"),
        "SEM008 should fire on grouped sp_std imports and macro usage"
    );
}

#[test]
fn sem008_ignores_comments_and_literals_without_hiding_following_code() {
    let code = r##"
/*
use sp_std::vec::Vec;
*/
fn build() {
    let quote = '"'; let _example = r#"sp_std::vec::Vec"#;
    use sp_std::vec::Vec;
}
"##;
    let diags = check_fixture("pallets/foo/src/lib.rs", code);
    let sem008_lines = diags
        .iter()
        .filter(|diagnostic| diagnostic.rule_id == "SEM008")
        .map(|diagnostic| diagnostic.line)
        .collect::<Vec<_>>();
    assert_eq!(
        sem008_lines,
        vec![7],
        "only the real sp_std import should be reported: {diags:?}"
    );
}

// ==========================================================================
// SEM009: Redundant contains_key before remove
// ==========================================================================

// ==========================================================================
// SEM010: ^ used as exponentiation (XOR bug)
// ==========================================================================

// ==========================================================================
// SEM011: Weight::zero() placeholder
// ==========================================================================
#[test]
fn sem011_detects_weight_zero_in_attribute() {
    let bad = include_str!("fixtures/bad_sem011.rs");
    let diags = check_fixture("pallets/foo/src/lib.rs", bad);
    assert!(
        has_rule(&diags, "SEM011"),
        "SEM011 should fire on Weight::zero() in weight attribute"
    );
}

#[test]
fn sem011_detects_literal_zero_weights() {
    let diags = check_fixture(
        "pallets/foo/src/lib.rs",
        include_str!("fixtures/bad_sem011.rs"),
    );
    let lines: Vec<_> = diags
        .iter()
        .filter(|d| d.rule_id == "SEM011")
        .map(|d| d.line)
        .collect();
    assert_eq!(lines, vec![6, 20, 27], "{diags:?}");
    let good = check_fixture(
        "pallets/foo/src/lib.rs",
        include_str!("fixtures/good_sem011.rs"),
    );
    assert!(!has_rule(&good, "SEM011"), "{good:?}");
}

#[test]
fn sem011_detects_weight_zero_in_qualified_frame_attribute() {
    let code = r#"
#[frame_support::pallet::weight(Weight::zero())]
pub fn submit(origin: OriginFor<T>) -> DispatchResult {
    let _ = origin;
    Ok(())
}
"#;
    let diags = check_fixture("pallets/foo/src/lib.rs", code);
    assert!(
        has_rule(&diags, "SEM011"),
        "SEM011 must inspect qualified FRAME weight attributes: {diags:?}"
    );
}

#[test]
fn sem011_respects_provable_cfg_attribute_truth() {
    let active = r#"
#[cfg_attr(not(any()), pallet::weight(Weight::zero()))]
pub fn active_weight() {}
"#;
    let inactive = r#"
#[cfg_attr(any(), pallet::weight(Weight::zero()))]
pub fn inactive_weight() {}
"#;
    let feature_dependent = r#"
#[cfg_attr(feature = "runtime-benchmarks", pallet::weight(Weight::zero()))]
pub fn feature_weight() {}
"#;

    assert!(
        has_rule(&check_fixture("pallets/foo/src/lib.rs", active), "SEM011"),
        "SEM011 must inspect a provably active cfg_attr weight"
    );
    assert!(
        !has_rule(&check_fixture("pallets/foo/src/lib.rs", inactive), "SEM011"),
        "SEM011 must not inspect a provably inactive cfg_attr weight"
    );
    assert!(
        !has_rule(
            &check_fixture("pallets/foo/src/lib.rs", feature_dependent),
            "SEM011"
        ),
        "SEM011 must not guess whether a Cargo feature enables cfg_attr"
    );
}

#[test]
fn sem011_allows_benchmarked_weight_in_attribute() {
    let good = include_str!("fixtures/good_sem011.rs");
    let diags = check_fixture("pallets/foo/src/lib.rs", good);
    assert!(
        !has_rule(&diags, "SEM011"),
        "SEM011 should NOT fire on benchmarked weight"
    );
}

// ==========================================================================
// SEM012: #[allow(dead_code)] in production code
// ==========================================================================
#[test]
fn sem012_detects_allow_dead_code() {
    let bad = include_str!("fixtures/bad_sem012.rs");
    let diags = check_fixture("pallets/foo/src/lib.rs", bad);
    assert!(
        has_rule(&diags, "SEM012"),
        "SEM012 should fire on #[allow(dead_code)] in pallet code"
    );
}

#[test]
fn sem012_allows_live_code() {
    let good = include_str!("fixtures/good_sem012.rs");
    let diags = check_fixture("pallets/foo/src/lib.rs", good);
    assert!(
        !has_rule(&diags, "SEM012"),
        "SEM012 should NOT fire when no dead_code suppression"
    );
}

#[test]
fn sem012_skips_test_files() {
    let bad = include_str!("fixtures/bad_sem012.rs");
    let diags = check_fixture("pallets/foo/src/tests.rs", bad);
    assert!(!has_rule(&diags, "SEM012"), "SEM012 should skip test files");
}

#[test]
fn sem012_ignores_inline_cfg_test_modules() {
    let code = r#"
pub fn live_code() {}

#[cfg(test)]
mod tests {
    #[allow(dead_code)]
    fn helper() {}
}
"#;
    let diags = check_fixture("pallets/foo/src/lib.rs", code);
    assert!(
        !has_rule(&diags, "SEM012"),
        "SEM012 should ignore #[allow(dead_code)] inside inline #[cfg(test)] modules"
    );
}

// ==========================================================================
// SEM013: Custom invalidity enums should use #[repr(u8)]
// ==========================================================================
#[test]
fn sem013_detects_missing_repr_u8() {
    let bad = include_str!("fixtures/bad_sem013.rs");
    let diags = check_fixture("pallets/foo/src/extension.rs", bad);
    assert!(
        has_rule(&diags, "SEM013"),
        "SEM013 should fire on custom invalidity enums without #[repr(u8)]"
    );
}

#[test]
fn sem013_allows_repr_u8() {
    let good = include_str!("fixtures/good_sem013.rs");
    let diags = check_fixture("pallets/foo/src/extension.rs", good);
    assert!(
        !has_rule(&diags, "SEM013"),
        "SEM013 should NOT fire when #[repr(u8)] is present"
    );
}

/// Production-only rules share one non-production masking layer. Each case
/// hides one copy of the finding and leaves a production copy that must still
/// be reported, so a mask that swallows everything also fails.
#[test]
fn production_rules_skip_non_production_cfg_but_keep_production_code() {
    let enum_def = "pub enum CustomInvalidity { InvalidAlias = 1 }";
    let cases = [
        (
            "crate-level cfg",
            format!("#![cfg(any(feature = \"runtime-benchmarks\", test))]\n{enum_def}"),
            0,
        ),
        (
            "test-helpers crate cfg",
            format!("#![cfg(any(feature = \"test-helpers\", test))]\n{enum_def}"),
            0,
        ),
        (
            "cfg(test) module",
            format!("#[cfg(test)]\nmod tests {{ {enum_def} }}\nmod prod {{ {enum_def} }}"),
            1,
        ),
        (
            "remote-test module",
            format!(
                "#[cfg(feature = \"remote-test\")]\nmod remote {{ {enum_def} }}\nmod prod {{ {enum_def} }}"
            ),
            1,
        ),
        (
            "integrity-test module",
            format!(
                "#[cfg(feature = \"integrity-test\")]\nmod checks {{ {enum_def} }}\nmod prod {{ {enum_def} }}"
            ),
            1,
        ),
        (
            "cfg(all(...)) module",
            format!(
                "#[cfg(all(feature = \"try-runtime\", feature = \"std\"))]\nmod checks {{ {enum_def} }}\nmod prod {{ {enum_def} }}"
            ),
            1,
        ),
        (
            "std-only module is production",
            format!("#[cfg(feature = \"std\")]\nmod native {{ {enum_def} }}"),
            1,
        ),
    ];
    for (case, code, expected) in cases {
        let diags = check_fixture("substrate/frame/example/src/lib.rs", &code);
        let count = diags.iter().filter(|d| d.rule_id == "SEM013").count();
        assert_eq!(count, expected, "{case}");
    }
}

#[test]
fn sem013_ignores_unrelated_invalidity_enum_names() {
    let code = r#"
enum RuntimeInvalidity {
    StaleProof,
    MissingState,
}
"#;
    let diags = check_fixture("pallets/foo/src/extension.rs", code);
    assert!(
        !has_rule(&diags, "SEM013"),
        "SEM013 applies only to the CustomInvalidity/CustomValidity convention: {diags:?}"
    );
}

// ==========================================================================
// SEM014: SubmitTransaction logs should use LOG_TARGET
// ==========================================================================
#[test]
fn sem014_detects_missing_log_target() {
    let bad = include_str!("fixtures/bad_sem014.rs");
    let diags = check_fixture("pallets/foo/src/lib.rs", bad);
    assert!(
        has_rule(&diags, "SEM014"),
        "SEM014 should fire when SubmitTransaction logging omits target: LOG_TARGET"
    );
}

#[test]
fn sem014_allows_multiline_log_target() {
    let good = include_str!("fixtures/good_sem014.rs");
    let diags = check_fixture("pallets/foo/src/lib.rs", good);
    assert!(
        !has_rule(&diags, "SEM014"),
        "SEM014 should NOT fire when target: LOG_TARGET is present on a following line"
    );
}

#[test]
fn sem014_ignores_unrelated_logs() {
    let code = r#"
fn log_other_issue() {
    log::warn!("background cleanup skipped");
}
"#;
    let diags = check_fixture("pallets/foo/src/lib.rs", code);
    assert!(
        !has_rule(&diags, "SEM014"),
        "SEM014 should ignore log macros unrelated to SubmitTransaction"
    );
}

#[test]
fn sem014_handles_wrapped_unsigned_submission_and_long_log_target() {
    let code = r#"
fn submit() {
    let transaction = SubmitTransaction::<T>::new(payload);
    transaction
        .submit_unsigned_transaction()
        .expect("submission succeeds");
    log::warn!(
        target: LOG_TARGET,
        "unable to submit transaction with a deliberately long and detailed diagnostic message",
    );
}
"#;
    let diags = check_fixture("pallets/foo/src/lib.rs", code);
    assert!(
        !has_rule(&diags, "SEM014"),
        "SEM014 should associate rustfmt-wrapped unsigned submission with its full log macro: {diags:?}"
    );
}

// ==========================================================================
// SEM015: #[pallet::authorize] should have #[pallet::weight_of_authorize]
// ==========================================================================
#[test]
fn sem015_detects_missing_weight_of_authorize() {
    let bad = include_str!("fixtures/bad_sem015.rs");
    let diags = check_fixture("pallets/foo/src/lib.rs", bad);
    assert!(
        has_rule(&diags, "SEM015"),
        "SEM015 should fire when #[pallet::authorize] has no companion #[pallet::weight_of_authorize]"
    );
}

#[test]
fn sem015_allows_weight_of_authorize() {
    let good = include_str!("fixtures/good_sem015.rs");
    let diags = check_fixture("pallets/foo/src/lib.rs", good);
    assert!(
        !has_rule(&diags, "SEM015"),
        "SEM015 should NOT fire when #[pallet::weight_of_authorize] is present"
    );
}

// ==========================================================================
// SEM016: CreateAuthorizedTransaction should include AuthorizeCall::new()
// ==========================================================================

// ==========================================================================
// TST006: Extrinsic without event
// ==========================================================================
#[test]
fn tst006_detects_extrinsic_without_event() {
    let bad = include_str!("fixtures/bad_tst006.rs");
    let diags = check_fixture("pallets/foo/src/lib.rs", bad);
    assert!(
        has_rule(&diags, "TST006"),
        "TST006 should fire on extrinsic that mutates storage without emitting event"
    );
}

#[test]
fn tst006_follows_always_called_pallet_helpers_one_level() {
    let code = r#"
#[pallet::call]
impl<T: Config> Pallet<T> {
    #[pallet::call_index(0)]
    pub fn via_self(origin: OriginFor<T>) -> DispatchResult {
        Value::<T>::put(1);
        Self::transition(1)
    }
    #[pallet::call_index(1)]
    pub fn via_pallet(origin: OriginFor<T>) -> DispatchResult {
        Value::<T>::put(2);
        Pallet::<T>::transition(2)
    }
    #[pallet::call_index(2)]
    pub fn via_path(origin: OriginFor<T>, xs: Vec<u32>) -> DispatchResult {
        Value::<T>::put(3);
        xs.into_iter().try_for_each(Self::transition)
    }
    #[pallet::call_index(3)]
    pub fn silent_helper(origin: OriginFor<T>) -> DispatchResult {
        Value::<T>::put(4);
        Self::store(4)
    }
    #[pallet::call_index(4)]
    pub fn two_levels(origin: OriginFor<T>) -> DispatchResult {
        Value::<T>::put(5);
        Self::indirect(5)
    }
    #[pallet::call_index(5)]
    pub fn via_let(origin: OriginFor<T>) -> DispatchResultWithPostInfo {
        Value::<T>::put(6);
        let _ = Self::transition(6)?;
        Ok(Self::transition(6)?.into())
    }
    // `add_trusted_validation_code`: only one branch emits.
    #[pallet::call_index(6)]
    pub fn one_branch(origin: OriginFor<T>, fresh: bool) -> DispatchResult {
        Value::<T>::put(7);
        if !fresh {
            Self::transition(7)?;
        }
        Ok(())
    }
    #[pallet::call_index(7)]
    pub fn function_value(origin: OriginFor<T>) -> DispatchResult {
        Value::<T>::put(8);
        let _f = Self::transition;
        Ok(())
    }
    #[pallet::call_index(8)]
    pub fn trait_helper(origin: OriginFor<T>) -> DispatchResult {
        Value::<T>::put(9);
        Self::notify(9)
    }
    #[pallet::call_index(9)]
    pub fn test_helper(origin: OriginFor<T>) -> DispatchResult {
        Value::<T>::put(10);
        Self::test_only(10)
    }
    // The helper only builds a closure that emits.
    #[pallet::call_index(10)]
    pub fn closure_helper(origin: OriginFor<T>) -> DispatchResult {
        Value::<T>::put(11);
        Self::deferred(11)
    }
}
impl<T: Config> Hooks<T> for Pallet<T> {
    fn notify(x: u32) -> DispatchResult {
        Self::deposit_event(Event::Changed { x });
        Ok(())
    }
}
#[cfg(test)]
impl<T: Config> Pallet<T> {
    fn test_only(x: u32) -> DispatchResult {
        Self::deposit_event(Event::Changed { x });
        Ok(())
    }
}
#[cfg(test)]
mod tests {
    impl<T: Config> Pallet<T> {
        fn store(x: u32) -> DispatchResult {
            Self::deposit_event(Event::Changed { x });
            Ok(())
        }
    }
}
impl<T: Config> Pallet<T> {
    fn transition(x: u32) -> DispatchResult {
        Self::deposit_event(Event::Changed { x });
        Ok(())
    }
    fn store(x: u32) -> DispatchResult {
        Value::<T>::put(x);
        Ok(())
    }
    fn indirect(x: u32) -> DispatchResult {
        Self::transition(x)
    }
    fn deferred(x: u32) -> impl Fn() {
        move || Self::deposit_event(Event::Changed { x })
    }
}
"#;
    let diags = check_fixture("pallets/foo/src/lib.rs", code);
    let mut names = diags
        .iter()
        .filter(|d| d.rule_id == "TST006")
        .map(|d| d.message.split('`').nth(1).unwrap().to_string())
        .collect::<Vec<_>>();
    names.sort();
    assert_eq!(
        names,
        vec![
            "closure_helper",
            "function_value",
            "one_branch",
            "silent_helper",
            "test_helper",
            "trait_helper",
            "two_levels",
            "via_path"
        ],
        "{diags:?}"
    );
}

#[test]
fn tst006_allows_extrinsic_with_event() {
    let good = include_str!("fixtures/good_tst006.rs");
    let diags = check_fixture("pallets/foo/src/lib.rs", good);
    assert!(
        !has_rule(&diags, "TST006"),
        "TST006 should NOT fire when extrinsic emits event"
    );
}

// ==========================================================================
// BEN003: Extrinsic without benchmark
// ==========================================================================
#[test]
fn ben003_detects_extrinsic_without_benchmark() {
    // BEN003 does cross-file analysis, so it needs a real benchmarking.rs sibling.
    // Using inline code that has #[pallet::call] but no sibling benchmark file.
    let bad = include_str!("fixtures/bad_ben003.rs");
    let diags = check_fixture("pallets/foo/src/lib.rs", bad);
    // Should fire because there's no benchmarking.rs next to this lib.rs
    assert!(
        has_rule(&diags, "BEN003"),
        "BEN003 should fire when no benchmarking.rs exists for pallet with extrinsics"
    );
}

#[test]
fn ben003_detects_qualified_pallet_call_attributes() {
    let code = r#"
#[frame_support::pallet::call]
impl<T: Config> Pallet<T> {
    #[frame_support::pallet::call_index(0)]
    pub fn submit(origin: OriginFor<T>) -> DispatchResult {
        let _ = ensure_signed(origin)?;
        Ok(())
    }
}

"#;
    let diags = check_fixture("pallets/foo/src/lib.rs", code);
    assert!(
        has_rule(&diags, "BEN003"),
        "BEN003 must recognize qualified FRAME pallet call attributes: {diags:?}"
    );
}

#[test]
fn ben003_recognizes_provably_active_cfg_pallet_call_attributes() {
    let active = r#"
#[cfg_attr(not(any()), pallet::call)]
impl<T: Config> Pallet<T> {
    pub fn submit(origin: OriginFor<T>) -> DispatchResult {
        let _ = ensure_signed(origin)?;
        Ok(())
    }
}
"#;
    let inactive = r#"
#[cfg_attr(any(), pallet::call)]
impl<T: Config> Pallet<T> {
    pub fn submit(origin: OriginFor<T>) -> DispatchResult {
        let _ = ensure_signed(origin)?;
        Ok(())
    }
}
"#;

    assert!(
        has_rule(&check_fixture("pallets/foo/src/lib.rs", active), "BEN003"),
        "BEN003 must inspect a provably active cfg_attr pallet call"
    );
    assert!(
        !has_rule(&check_fixture("pallets/foo/src/lib.rs", inactive), "BEN003"),
        "BEN003 must not treat a provably inactive cfg_attr pallet call as active"
    );
}

#[test]
fn ben003_detects_implicit_index_dispatchables() {
    let code = r#"
#[pallet::call]
impl<T: Config> Pallet<T> {
    pub fn submit(origin: OriginFor<T>) -> DispatchResult {
        let _ = ensure_signed(origin)?;
        Ok(())
    }
}
"#;
    let diags = check_fixture("pallets/foo/src/lib.rs", code);
    assert!(
        has_rule(&diags, "BEN003"),
        "BEN003 must inspect FRAME dispatchables without explicit call indices: {diags:?}"
    );
}

#[test]
fn ben003_skips_non_pallet_files() {
    let good = include_str!("fixtures/good_ben003.rs");
    let diags = check_fixture("pallets/foo/src/lib.rs", good);
    assert!(
        !has_rule(&diags, "BEN003"),
        "BEN003 should NOT fire on lib.rs without #[pallet::call]"
    );
}

#[test]
fn ben003_allows_extrinsic_with_matching_benchmark_file() {
    let root = std::env::temp_dir().join(format!(
        "polkadot-linter-ben003-match-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let pallet_dir = root.join("pallets/foo/src");
    std::fs::create_dir_all(&pallet_dir).unwrap();

    let lib_path = pallet_dir.join("lib.rs");
    let bench_path = pallet_dir.join("benchmarking.rs");
    let lib = r#"
#[pallet::call]
impl<T: Config> Pallet<T> {
    #[pallet::call_index(0)]
    pub fn submit(origin: OriginFor<T>) -> DispatchResult {
        let _ = ensure_signed(origin)?;
        Ok(())
    }
}
"#;
    let benches = r#"
#[benchmark]
fn submit() {}
"#;

    std::fs::write(&lib_path, lib).unwrap();
    std::fs::write(&bench_path, benches).unwrap();

    let diags = check_fixture_path(lib_path, lib);
    assert!(
        !has_rule(&diags, "BEN003"),
        "BEN003 should not fire when the extrinsic has a sibling benchmark"
    );
}

#[test]
fn ben003_allows_benchmark_variants_for_one_extrinsic() {
    let root = std::env::temp_dir().join(format!(
        "polkadot-linter-ben003-variants-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let pallet_dir = root.join("pallets/foo/src");
    std::fs::create_dir_all(&pallet_dir).unwrap();

    let lib_path = pallet_dir.join("lib.rs");
    let bench_path = pallet_dir.join("benchmarking.rs");
    let lib = r#"
#[pallet::call]
impl<T: Config> Pallet<T> {
    #[pallet::call_index(0)]
    pub fn unload_recycler_into_coins(origin: OriginFor<T>) -> DispatchResult {
        let _ = ensure_signed(origin)?;
        Ok(())
    }
}
"#;
    let benches = r#"
#[benchmark]
fn unload_recycler_into_coins_1_2() {}

#[benchmark]
fn unload_recycler_into_coins_3_8() {}
"#;

    std::fs::write(&lib_path, lib).unwrap();
    std::fs::write(&bench_path, benches).unwrap();

    let diags = check_fixture_path(lib_path, lib);
    assert!(
        !has_rule(&diags, "BEN003"),
        "BEN003 should treat benchmark variants as coverage for the extrinsic"
    );
}

// ==========================================================================
// SEC001: Unbounded Vec in extrinsic params
// ==========================================================================

// ==========================================================================
// SEC002: debug_assert in production code
// ==========================================================================

// ==========================================================================
// SEC003: Missing decode depth limit
// ==========================================================================

// ==========================================================================
// SEC004: Unsafe arithmetic in weight attributes
// ==========================================================================

// ==========================================================================
// SEC005: Expensive operations in weight calculation
// ==========================================================================

// ==========================================================================
// SEC006: Unchecked repatriate_reserved return value
// ==========================================================================

// ==========================================================================
// SEC007: let _ = swallowing Result
// ==========================================================================

// ==========================================================================
// SEC008: Panic in production code
// ==========================================================================

// ==========================================================================
// SEC009: Raw arithmetic in fallible functions
// ==========================================================================

// ==========================================================================
// VAL003: Storage write before validation
// ==========================================================================

// ==========================================================================
// SEC010: Missing transactional in hook
// ==========================================================================

// ==========================================================================
// SEC011: Storage iteration in dispatchables/hooks
// ==========================================================================

// ==========================================================================
// SEC012: Unbounded clear_prefix
// ==========================================================================

// ==========================================================================
// MOK001: Excessive mock setup
// ==========================================================================
#[test]
fn mok001_detects_mock_heavy_test_setup() {
    let bad = include_str!("fixtures/bad_mok001.rs");
    let diags = check_fixture("tests/mock_usage.rs", bad);
    assert!(
        has_rule(&diags, "MOK001"),
        "MOK001 should fire on mock-heavy tests"
    );
}

#[test]
fn mok001_allows_outcome_focused_tests() {
    let good = include_str!("fixtures/good_mok001.rs");
    let diags = check_fixture("tests/mock_usage.rs", good);
    assert!(
        !has_rule(&diags, "MOK001"),
        "MOK001 should not fire on outcome-focused tests"
    );
}

// ==========================================================================
// SEC013: Unbounded storage collections
// ==========================================================================

// ==========================================================================
// SEC014: Identity hasher on common key types
// ==========================================================================

// ==========================================================================
// SEC015: dispatch_bypass_filter in production
// ==========================================================================

// ==========================================================================
// SEC016: Missing StorageVersion check in runtime upgrade
// ==========================================================================

// ==========================================================================
// SEC017: Vec<T> in pallet events
// ==========================================================================

// ==========================================================================
// SEC018: Missing weight term for unbounded input length
// ==========================================================================

// ==========================================================================
// Config severity override
// ==========================================================================
#[test]
fn config_severity_override_works() {
    let mut config = all_rules_config();
    config
        .rules
        .severity
        .insert("SEM003".to_string(), "error".to_string());

    let bad = include_str!("fixtures/bad_sem003.rs");
    let diags = check_fixture_with_config("src/lib.rs", bad, &config);
    let sem003 = diags.iter().find(|d| d.rule_id == "SEM003");
    assert!(sem003.is_some(), "SEM003 should fire");
    assert_eq!(
        sem003.unwrap().severity,
        polkadot_linter::diagnostics::Severity::Error,
        "Severity should be overridden to Error"
    );
}

#[test]
fn config_rule_disable_works() {
    let mut config = polkadot_linter::config::Config::default();
    config.rules.enabled.insert("SEM003".to_string(), false);

    let bad = include_str!("fixtures/bad_sem003.rs");
    let diags = check_fixture_with_config("src/lib.rs", bad, &config);
    assert!(!has_rule(&diags, "SEM003"), "SEM003 should be disabled");
}

// === BEN discovery and recall guard (fix/benchmark-discovery) ===
#[test]
fn ben_discovery_v1_v2_calls_and_missing_benchmarks() {
    // Run each syntax separately: unrelated benchmarks must never hide a
    // missing weight or dispatchable, even when names collide across crates.
    for (syntax, source) in [
        (
            "v1",
            r#"frame_benchmarking::benchmarks_instance_pallet! {
            expensive_variant { let x = 1; }: actual_call(RawOrigin::Root, x)
            verify { assert_eq!(x, 1); }
            unverified { let x = 1; }: _(RawOrigin::Root, x)
        }"#,
        ),
        (
            "v2",
            r#"#[benchmarks(instance)] mod benchmarks {
            fn helper() {}
            #[benchmark]
            fn expensive_variant(n: Linear<0, { T::Limit::get() }>) {
                #[extrinsic_call]
                actual_call(RawOrigin::Root, n);
                assert_last_event::<T>(Event::Done.into());
            }
            #[benchmark]
            fn unverified() {
                assert!(setup());
                #[block]
                { let _ = Pallet::<T>::unverified(RawOrigin::Root); }
            }
        }"#,
        ),
    ] {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("src");
        std::fs::create_dir_all(src.join("benchmarking")).unwrap();
        std::fs::write(src.join("benchmarking/mod.rs"), source).unwrap();
        let weights = "pub trait WeightInfo { fn expensive_variant(); fn missing_weight(); }";
        let weights_path = src.join("weights.rs");
        std::fs::write(&weights_path, weights).unwrap();
        let weight_diags = check_fixture(weights_path.to_str().unwrap(), weights);
        let ben001 = weight_diags
            .iter()
            .filter(|d| d.rule_id == "BEN001")
            .collect::<Vec<_>>();
        assert_eq!(ben001.len(), 1, "{syntax}: {ben001:?}");
        assert!(ben001[0].message.contains("missing_weight"));
        let calls = r#"#[pallet::call] impl<T: Config> Pallet<T> {
            pub fn actual_call() {} pub fn missing_call() {}
        }"#;
        let lib_path = src.join("lib.rs");
        std::fs::write(&lib_path, calls).unwrap();
        let call_diags = check_fixture(lib_path.to_str().unwrap(), calls);
        let ben003 = call_diags
            .iter()
            .filter(|d| d.rule_id == "BEN003")
            .collect::<Vec<_>>();
        assert_eq!(ben003.len(), 1, "{syntax}: {ben003:?}");
        assert!(ben003[0].message.contains("missing_call"));
        let verification = check_fixture("pallets/foo/src/benchmarking/nested.rs", source);
        let ben002 = verification
            .iter()
            .filter(|d| d.rule_id == "BEN002")
            .collect::<Vec<_>>();
        assert_eq!(ben002.len(), 1, "{syntax}: {ben002:?}");
        assert!(ben002[0].message.contains("unverified"));
    }
}

#[test]
fn ben_discovers_inline_and_owned_companion_benchmarks() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("Cargo.toml"),
        r#"[workspace]
[workspace.dependencies]
pallet-demo = { path = "pallet" }
bench-demo = { path = "benchmarking" }
"#,
    )
    .unwrap();
    for (folder, name, dependency) in [
        ("pallet", "pallet-demo", ""),
        (
            "benchmarking",
            "pallet-demo-benchmarking",
            "pallet-demo = { workspace = true }",
        ),
        ("unrelated", "unrelated-benchmarking", ""),
    ] {
        let root = dir.path().join(folder);
        std::fs::create_dir_all(root.join("src")).unwrap();
        std::fs::write(
            root.join("Cargo.toml"),
            format!(
                "[package]\nname = \"{name}\"\nversion = \"0.1.0\"\n[dependencies]\n{dependency}\n"
            ),
        )
        .unwrap();
    }
    std::fs::write(
        dir.path().join("benchmarking/src/lib.rs"),
        r#"
        #[cfg(feature = "runtime-benchmarks")]
        mod benches { #[benchmark] fn external() { #[extrinsic_call] covered(); assert!(true); } }
    "#,
    )
    .unwrap();
    std::fs::write(
        dir.path().join("unrelated/src/lib.rs"),
        "#[benchmark] fn missing() {}",
    )
    .unwrap();
    let decoy = dir.path().join("pallet/benchmarking-extra");
    std::fs::create_dir_all(decoy.join("src")).unwrap();
    std::fs::write(decoy.join("Cargo.toml"), "[package]\nname = \"pallet-other-benchmarking\"\nversion = \"0.1.0\"\n[dependencies]\npallet-demo = { path = \"..\" }\n").unwrap();
    std::fs::write(decoy.join("src/lib.rs"), "#[benchmark] fn missing() {}").unwrap();
    let calls = r#"#[pallet::call] impl<T: Config> Pallet<T> {
        pub fn covered() {} pub fn inline() {} pub fn missing() {}
    }
    #[cfg(feature = "runtime-benchmarks")]
    mod benches { frame_benchmarking::benchmarks! { inline {}: _(RawOrigin::Root) verify {} } }
    "#;
    let lib = dir.path().join("pallet/src/lib.rs");
    std::fs::write(&lib, calls).unwrap();
    let diags = check_fixture(lib.to_str().unwrap(), calls);
    let missing = diags
        .iter()
        .filter(|d| d.rule_id == "BEN003")
        .collect::<Vec<_>>();
    assert_eq!(missing.len(), 1, "{missing:?}");
    assert!(missing[0].message.contains("missing"));
    // The same ownership relationship works without workspace dependencies.
    std::fs::remove_file(dir.path().join("Cargo.toml")).unwrap();
    std::fs::write(
        dir.path().join("benchmarking/Cargo.toml"),
        r#"[package]
name = "pallet-demo-benchmarking"
version = "0.1.0"
[dependencies]
pallet-demo = { path = "../pallet" }
"#,
    )
    .unwrap();
    let direct = check_fixture(lib.to_str().unwrap(), calls);
    assert_eq!(direct.iter().filter(|d| d.rule_id == "BEN003").count(), 1);
}

#[test]
fn ben_ignores_docs_tests_and_examples_but_retains_runtime_benchmarks() {
    let docs = r#"
/// #[pallet::call_index(0)]
/// pub fn documented_call() {}
/// #[benchmark]
/// fn documented_bench() {}
#[cfg(all(test, feature = "runtime-benchmarks"))] mod tests {
    #[pallet::call] impl<T: Config> Pallet<T> { pub fn test_call() {} }
    #[benchmark] fn test_bench() {}
}
#[cfg(feature = "runtime-benchmarks")]
mod benchmarks { #[benchmark] fn real_bench() {} }
"#;
    let diags = check_fixture("pallets/foo/src/lib.rs", docs);
    assert!(!has_rule(&diags, "BEN003"), "{diags:?}");
    let benches = diags
        .iter()
        .filter(|d| d.rule_id == "BEN002")
        .collect::<Vec<_>>();
    assert_eq!(benches.len(), 1);
    assert!(benches[0].message.contains("real_bench"));
    assert!(!has_rule(
        &check_fixture(
            "pallets/integration-tests/src/weights.rs",
            "trait WeightInfo { fn fake(); }"
        ),
        "BEN001"
    ));
}

#[test]
fn ben003_recognizes_qualified_self_pallet_calls() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("benchmarking.rs"),
        r#"use crate::Pallet as CollatorSelection;
        benchmarks! {
            set_desired_candidates { }: {
                <CollatorSelection<T>>::set_desired_candidates(RawOrigin::Root.into(), 1)?;
                <T as Config>::set_invulnerables(RawOrigin::Root.into())?;
            } verify { assert!(true); }
        }"#,
    )
    .unwrap();
    let code = "#[pallet::call] impl Pallet { pub fn set_desired_candidates() {} pub fn set_invulnerables() {} }";
    let path = dir.path().join("lib.rs");
    std::fs::write(&path, code).unwrap();
    let ben003 = check_fixture(path.to_str().unwrap(), code)
        .into_iter()
        .filter(|d| d.rule_id == "BEN003")
        .collect::<Vec<_>>();
    assert_eq!(ben003.len(), 1, "{ben003:?}");
    assert!(ben003[0].message.contains("set_invulnerables"));
}

/// A workspace with `pallet-demo` at `pallet_dir` and a runtime crate that
/// depends on it and hosts orml `runtime_benchmarks!` blocks.
fn ben_workspace(pallet_dir: &str, runtime_benchmarks: &str) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    std::fs::write(
        root.join("Cargo.toml"),
        format!("[workspace]\nmembers = [\"{pallet_dir}\", \"runtime/*\"]\n[workspace.dependencies]\npallet-demo = {{ path = \"{pallet_dir}\" }}\n"),
    )
    .unwrap();
    std::fs::create_dir_all(root.join(pallet_dir).join("src")).unwrap();
    std::fs::write(
        root.join(pallet_dir).join("Cargo.toml"),
        "[package]\nname = \"pallet-demo\"\nversion = \"0.1.0\"\n",
    )
    .unwrap();
    let runtime = root.join("runtime/demo");
    std::fs::create_dir_all(runtime.join("src/benchmarking")).unwrap();
    std::fs::write(
        runtime.join("Cargo.toml"),
        "[package]\nname = \"demo-runtime\"\nversion = \"0.1.0\"\n[dependencies]\npallet-demo = { workspace = true }\n",
    )
    .unwrap();
    std::fs::write(runtime.join("src/lib.rs"), "mod benchmarking;").unwrap();
    std::fs::write(runtime.join("src/benchmarking/mod.rs"), "mod demo;").unwrap();
    std::fs::write(runtime.join("src/benchmarking/demo.rs"), runtime_benchmarks).unwrap();
    dir
}

#[test]
fn ben_discovers_benchmarks_hosted_in_a_dependent_runtime() {
    let dir = ben_workspace(
        "pallets/demo",
        r#"use orml_benchmarking::runtime_benchmarks;
        runtime_benchmarks! {
            { Runtime, pallet_demo }
            hosted { let x = 1; }: _(RawOrigin::Root, x)
            verify { assert_eq!(x, 1); }
        }
        runtime_benchmarks! {
            { Runtime, pallet_other }
            missing { }: _(RawOrigin::Root)
        }
        runtime_benchmarks! {
            { Runtime, pallet_demo }
            renamed { }: hosted(RawOrigin::Root)
            by_name { }: { Demo::named(RawOrigin::Root.into())? }
            by_alias { }: { DemoPallet::<Runtime>::aliased(RawOrigin::Root.into())? }
            by_other { }: { Other::unmeasured(RawOrigin::Root.into())? }
        }
        type DemoPallet<T> = pallet_demo::Pallet<T>;
        use crate::{Demo, Other};"#,
    );
    // Orphan files and test-only modules host nothing.
    let runtime = dir.path().join("runtime/demo/src");
    let unmeasured = |name: &str| {
        format!(
            "runtime_benchmarks! {{ {{ Runtime, pallet_demo }} {name} {{ }}: _(RawOrigin::Root) }}"
        )
    };
    std::fs::write(
        runtime.join("benchmarking/orphan.rs"),
        unmeasured("orphaned"),
    )
    .unwrap();
    std::fs::write(runtime.join("benchmarking/gated.rs"), unmeasured("gated")).unwrap();
    std::fs::write(
        runtime.join("benchmarking/mod.rs"),
        "mod demo; #[cfg(test)] mod gated;",
    )
    .unwrap();
    std::fs::write(
        runtime.join("lib.rs"),
        "mod benchmarking; construct_runtime!(pub enum Runtime { Demo: pallet_demo = 1, Other: pallet_other = 2 });",
    )
    .unwrap();
    let src = dir.path().join("pallets/demo/src");
    let calls = "#[pallet::call] impl<T: Config> Pallet<T> { pub fn hosted() {} pub fn missing() {} pub fn renamed() {} pub fn orphaned() {} pub fn gated() {} pub fn named() {} pub fn aliased() {} pub fn unmeasured() {} }";
    std::fs::write(src.join("lib.rs"), calls).unwrap();
    let ben003 = check_fixture(src.join("lib.rs").to_str().unwrap(), calls)
        .into_iter()
        .filter(|d| d.rule_id == "BEN003")
        .collect::<Vec<_>>();
    // `renamed` measures `hosted`, so it does not cover `renamed`.
    let mut names = ben003
        .iter()
        .map(|d| d.message.split('`').nth(1).unwrap())
        .collect::<Vec<_>>();
    names.sort();
    assert_eq!(
        names,
        ["gated", "missing", "orphaned", "renamed", "unmeasured"],
        "{ben003:?}"
    );

    let weights = "pub trait WeightInfo { fn hosted() -> Weight; fn missing() -> Weight; fn renamed() -> Weight; }";
    std::fs::write(src.join("lib.rs"), "pub mod weights;").unwrap();
    std::fs::write(src.join("weights.rs"), weights).unwrap();
    let ben001 = check_fixture(src.join("weights.rs").to_str().unwrap(), weights)
        .into_iter()
        .filter(|d| d.rule_id == "BEN001")
        .collect::<Vec<_>>();
    assert_eq!(ben001.len(), 1, "{ben001:?}");
    assert!(ben001[0].message.contains("`missing`"));
}

// === End BEN discovery and recall guard ===

// === BEN measured-call binding regression ===
#[test]
fn ben003_resolves_dispatched_call_bindings_without_counting_setup_calls() {
    for source in [
        r#"frame_benchmarking::benchmarks_instance_pallet! {
            renamed {}: { assert_ok!(Pallet::<T>::actual(RawOrigin::Root)); }
            verify {}
        }"#,
        r#"frame_benchmarking::benchmarks! {
            renamed {
                let c in 0 .. 10;
                Pallet::<T>::setup_only();
                let call = Call::<T>::actual {};
            }: { call.dispatch_bypass_filter(origin)? }
            verify {}
        }"#,
        r#"#[benchmarks] mod benchmarks {
            #[benchmark] fn renamed() {
                Pallet::<T>::setup_only();
                let call = Call::<T>::actual {};
                #[block] { call.dispatch_bypass_filter(origin)?; }
                assert!(true);
            }
            // A same-name benchmark measuring a different call is not evidence.
            #[benchmark] fn misleading() {
                #[extrinsic_call] actual();
            }
        }"#,
    ] {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("benchmarking.rs"), source).unwrap();
        let calls = r#"#[pallet::call] impl<T: Config> Pallet<T> {
            pub fn actual() {} pub fn setup_only() {} pub fn misleading() {}
        }"#;
        let path = dir.path().join("lib.rs");
        std::fs::write(&path, calls).unwrap();
        let diags = check_fixture(path.to_str().unwrap(), calls);
        let missing = diags
            .iter()
            .filter(|d| d.rule_id == "BEN003")
            .collect::<Vec<_>>();
        assert_eq!(missing.len(), 2, "{missing:?}");
        assert!(missing.iter().all(|d| !d.message.contains("`actual`")));
    }
}
// === End BEN measured-call binding regression ===

// Round 2: exercise actual Cargo module trees and scan lifetimes.
#[test]
fn ben_scan_under_tests_directory_and_example_library_crates() {
    use polkadot_linter::engine::LintEngine;
    let dir = tempfile::tempdir().unwrap();
    for folder in ["tests/ws", "substrate/frame/examples/demo"] {
        let root = dir.path().join(folder);
        std::fs::create_dir_all(root.join("src")).unwrap();
        std::fs::write(
            root.join("Cargo.toml"),
            "[package]\nname = \"demo\"\nversion = \"0.1.0\"\n",
        )
        .unwrap();
        std::fs::write(root.join("src/lib.rs"), "mod weights; #[pallet::call] impl Pallet { pub fn missing() {} } #[benchmark] fn unverified() { #[block] {} }").unwrap();
        std::fs::write(
            root.join("src/weights.rs"),
            "trait WeightInfo { fn missing(); }",
        )
        .unwrap();
        let mut engine = LintEngine::new(all_rules_config());
        engine.filter_rules(&["BEN".into()]);
        let findings = engine.scan(&root);
        for rule in ["BEN001", "BEN002", "BEN003"] {
            assert!(has_rule(&findings, rule), "{folder} {rule}: {findings:?}");
        }
        // A real Cargo example target, including a custom path, is excluded.
        std::fs::write(root.join("Cargo.toml"), "[package]\nname = \"demo\"\nversion = \"0.1.0\"\n[[example]]\nname = \"custom\"\npath = \"custom/lib.rs\"\n").unwrap();
        std::fs::create_dir_all(root.join("custom")).unwrap();
        std::fs::create_dir_all(root.join("examples")).unwrap();
        let code =
            "#[pallet::call] impl Pallet { pub fn missing() {} } #[benchmark] fn unverified() {}";
        std::fs::write(root.join("custom/lib.rs"), code).unwrap();
        std::fs::write(root.join("examples/demo.rs"), code).unwrap();
        let findings = engine.scan(&root);
        assert!(
            findings
                .iter()
                .all(|d| !d.file.starts_with(root.join("examples"))
                    && !d.file.starts_with(root.join("custom"))),
            "{findings:?}"
        );
    }
}

#[test]
fn ben_module_ancestry_and_cache_refresh() {
    use polkadot_linter::engine::LintEngine;
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    std::fs::create_dir(root.join("src")).unwrap();
    std::fs::create_dir(root.join("src/nested")).unwrap();
    std::fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"demo\"\nversion = \"0.1.0\"\n",
    )
    .unwrap();
    std::fs::write(
        root.join("src/lib.rs"),
        r#"
        mod weights;
        #[cfg(test)] mod test_benches;
        #[cfg(feature = "runtime-benchmarks")] #[path = "nested/../live.rs"] mod benchmarking;
        #[pallet::call] impl Pallet { pub fn dead() {} pub fn test_only() {} pub fn live() {} }
    "#,
    )
    .unwrap();
    std::fs::write(
        root.join("src/weights.rs"),
        "trait WeightInfo { fn dead(); fn test_only(); fn live(); }",
    )
    .unwrap();
    for (file, name) in [
        ("old_benches.rs", "dead"),
        ("test_benches.rs", "test_only"),
        ("live.rs", "live"),
    ] {
        std::fs::write(
            root.join("src").join(file),
            format!("#[benchmark] fn {name}() {{ #[extrinsic_call] _(); }}"),
        )
        .unwrap();
    }
    let mut engine = LintEngine::new(all_rules_config());
    engine.filter_rules(&["BEN".into()]);
    let findings = engine.scan(root);
    for rule in ["BEN001", "BEN003"] {
        let missing = findings
            .iter()
            .filter(|d| d.rule_id == rule)
            .collect::<Vec<_>>();
        assert_eq!(missing.len(), 2, "{missing:?}");
        assert!(missing.iter().any(|d| d.message.contains("`dead`")));
        assert!(missing.iter().any(|d| d.message.contains("`test_only`")));
    }
    assert_eq!(
        findings.iter().filter(|d| d.rule_id == "BEN002").count(),
        1,
        "{findings:?}"
    );
    std::fs::write(root.join("src/live.rs"), "").unwrap();
    let findings = engine.scan(root);
    assert_eq!(findings.iter().filter(|d| d.rule_id == "BEN003").count(), 3);
}

#[test]
fn ben003_requires_owning_receiver_and_dispatch() {
    for syntax in ["v1", "v2"] {
        for measured in [
            "OtherPallet::<T>::transfer()",
            "T::Currency::transfer()",
            "Value::<T>::transfer()",
            "Foo::<T>::transfer()",
            "Call::<T>::transfer {}",
            "{ let call = Call::<T>::transfer {}; }",
            "foreign.dispatch(origin)",
        ] {
            let dir = tempfile::tempdir().unwrap();
            let body = format!("let foreign = OtherCall::<T>::transfer {{}}; {measured};");
            let bench = if syntax == "v1" {
                format!("benchmarks! {{ renamed {{ let foreign = OtherCall::<T>::transfer {{}}; }}: {{ {body} }} verify {{ assert!(true); }} }}")
            } else {
                format!("#[benchmark] fn renamed() {{ let foreign = OtherCall::<T>::transfer {{}}; #[block] {{ {body} }} assert!(true); }}")
            };
            std::fs::write(dir.path().join("benchmarking.rs"), bench).unwrap();
            let code = "#[pallet::call] impl Pallet { pub fn transfer() {} }";
            let path = dir.path().join("lib.rs");
            std::fs::write(&path, code).unwrap();
            assert!(
                has_rule(&check_fixture(path.to_str().unwrap(), code), "BEN003"),
                "{syntax}: {measured}"
            );
        }
    }
    // Names overlapping storage and other pallets never confer ownership.
    let source = r#"use other_pallet::Pallet as Other;
        type Storage<T> = Value<T>;
        #[benchmark] fn set_variant() { #[block] { Other::<T>::set(); Storage::<T>::set(); T::Currency::set(); } }
    "#;
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("benchmarking.rs"), source).unwrap();
    let code = "#[pallet::call] impl Pallet { pub fn set() {} }";
    let path = dir.path().join("lib.rs");
    std::fs::write(&path, code).unwrap();
    assert!(has_rule(
        &check_fixture(path.to_str().unwrap(), code),
        "BEN003"
    ));
}

#[test]
fn ben003_resolves_owning_aliases_and_assertion_arguments() {
    for expression in [
        "Pallet::<T>::transfer()",
        "Self::transfer()",
        "Membership::<T>::transfer()",
        "Local::<T>::transfer()",
        "assert!(Membership::<T>::transfer().is_ok(),)",
        "assert_eq!(Membership::<T>::transfer(), Ok(()),)",
        "assert_ne!(Local::<T>::transfer(), Err(()),)",
        "debug_assert!(Local::<T>::transfer().is_ok(),)",
        "debug_assert_eq!(Local::<T>::transfer(), Ok(()),)",
        "debug_assert_ne!(Local::<T>::transfer(), Err(()),)",
    ] {
        for syntax in ["v1", "v2"] {
            let dir = tempfile::tempdir().unwrap();
            let measured = format!("{{ {expression}; }}");
            let source = if syntax == "v1" {
                format!("use super::Pallet as Membership; type Local<T> = Membership<T>; benchmarks! {{ renamed {{}}: {measured} verify {{ assert!(true); }} }}")
            } else {
                format!("use super::Pallet as Membership; type Local<T> = Membership<T>; #[benchmark] fn renamed() {{ #[block] {measured} assert!(true); }}")
            };
            std::fs::write(dir.path().join("benchmarking.rs"), source).unwrap();
            let code = "#[pallet::call] impl Pallet { pub fn transfer() {} }";
            let path = dir.path().join("lib.rs");
            std::fs::write(&path, code).unwrap();
            assert!(
                !has_rule(&check_fixture(path.to_str().unwrap(), code), "BEN003"),
                "{syntax}: {expression}"
            );
        }
    }
}

#[test]
fn ben002_empty_verify_is_not_verification() {
    let code = "benchmarks! { empty {}: _() verify { /* no check */ } nonempty {}: _() verify { check_state(); } }";
    let findings = check_fixture("pallets/demo/src/benchmarking.rs", code);
    let findings = findings
        .iter()
        .filter(|d| d.rule_id == "BEN002")
        .collect::<Vec<_>>();
    assert_eq!(findings.len(), 1, "{findings:?}");
    assert!(findings[0].message.contains("`empty`"));
}

#[test]
fn ben003_foreign_imports_can_shadow_conventional_names() {
    for source in [
        "use other_pallet::Pallet; #[benchmark] fn renamed() { #[block] { Pallet::<T>::transfer(); } }",
        "use other_pallet::Call; #[benchmark] fn renamed() { let call = Call::<T>::transfer {}; #[block] { call.dispatch(origin); } }",
    ] {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("benchmarking.rs"), source).unwrap();
        let code = "#[pallet::call] impl Pallet { pub fn transfer() {} }";
        let path = dir.path().join("lib.rs");
        std::fs::write(&path, code).unwrap();
        assert!(has_rule(&check_fixture(path.to_str().unwrap(), code), "BEN003"), "{source}");
    }
}

fn check_ben003_binding_body(body: &str, missing: bool) {
    for syntax in ["v1", "v2"] {
        let dir = tempfile::tempdir().unwrap();
        let source = if syntax == "v1" {
            format!("benchmarks! {{ renamed {{}}: {{ {body} }} verify {{ assert!(true); }} }}")
        } else {
            format!("#[benchmark] fn renamed() {{ {body} assert!(true); }}")
        };
        std::fs::write(dir.path().join("benchmarking.rs"), source).unwrap();
        let code = "#[pallet::call] impl Pallet { pub fn transfer() {} }";
        let path = dir.path().join("lib.rs");
        std::fs::write(&path, code).unwrap();
        assert_eq!(
            has_rule(&check_fixture(path.to_str().unwrap(), code), "BEN003"),
            missing,
            "{syntax}: {body}"
        );
    }
}

#[test]
fn ben003_later_call_binding_does_not_credit_dispatch() {
    check_ben003_binding_body("let call = other::Call::<T>::transfer {}; #[block] { call.dispatch(origin); } let call = Call::<T>::transfer {};", true);
}

#[test]
fn ben003_latest_call_binding_wins() {
    check_ben003_binding_body("let call = Call::<T>::transfer {}; #[block] { let call = other::Call::<T>::transfer {}; call.dispatch(origin); }", true);
    check_ben003_binding_body("let call = Call::<T>::transfer {}; let call = other::Call::<T>::transfer {}; #[block] { call.dispatch(origin); }", true);
    check_ben003_binding_body("let call = other::Call::<T>::transfer {}; let call = Call::<T>::transfer {}; #[block] { call.dispatch(origin); }", false);
}

#[test]
fn ben003_construct_and_dispatch_inside_measured_block() {
    check_ben003_binding_body(
        "#[block] { let call = Call::<T>::transfer {}; call.dispatch(origin); }",
        false,
    );
}

#[test]
fn ben001_follows_custom_lib_root_sibling_modules() {
    use polkadot_linter::engine::LintEngine;
    let dir = tempfile::tempdir().unwrap();
    let src = dir.path().join("src");
    std::fs::create_dir(&src).unwrap();
    std::fs::write(dir.path().join("Cargo.toml"), "[package]\nname = \"custom-root\"\nversion = \"0.1.0\"\n[lib]\npath = \"src/runtime.rs\"\n").unwrap();
    std::fs::write(
        src.join("runtime.rs"),
        "pub mod weights; pub mod benchmarking;",
    )
    .unwrap();
    std::fs::write(
        src.join("weights.rs"),
        "pub trait WeightInfo { fn missing(); }",
    )
    .unwrap();
    std::fs::write(src.join("benchmarking.rs"), "").unwrap();
    let mut engine = LintEngine::new(all_rules_config());
    engine.filter_rules(&["BEN001".into()]);
    let findings = engine.scan(dir.path());
    assert_eq!(findings.len(), 1, "{findings:?}");
    assert!(findings[0].message.contains("`missing`"));
}

#[test]
fn ben003_resolves_function_and_block_local_imports() {
    for body in [
        "use crate::Pallet as Local; #[block] { Local::<T>::transfer(); }",
        "#[block] { use crate::Pallet as Local; Local::<T>::transfer(); }",
        "use other::Pallet as Local; #[block] { use crate::Pallet as Local; Local::<T>::transfer(); }",
    ] {
        check_ben003_binding_body(body, false);
    }
    check_ben003_binding_body("use crate::Pallet as Local; #[block] { use other::Pallet as Local; Local::<T>::transfer(); }", true);
}

// ==========================================================================
// Runtime configuration rules (RTC001-RTC008)
// ==========================================================================

fn rtc_lines(diags: &[polkadot_linter::diagnostics::Diagnostic], rule_id: &str) -> Vec<usize> {
    let mut lines = diags
        .iter()
        .filter(|diagnostic| diagnostic.rule_id == rule_id)
        .map(|diagnostic| diagnostic.line)
        .collect::<Vec<_>>();
    lines.sort_unstable();
    lines
}

fn rtc_findings(diags: &[polkadot_linter::diagnostics::Diagnostic]) -> Vec<String> {
    diags
        .iter()
        .filter(|diagnostic| diagnostic.rule_id.starts_with("RTC"))
        .map(|diagnostic| format!("{}:{}", diagnostic.rule_id, diagnostic.line))
        .collect()
}

/// Write a minimal runtime crate (manifest + lib.rs + extra files) to a temp dir.
fn rtc_runtime_crate(package: &str, files: &[(&str, &str)]) -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("temporary runtime crate should be created");
    std::fs::write(
        dir.path().join("Cargo.toml"),
        format!("[package]\nname = \"{package}\"\nversion = \"0.1.0\"\nedition = \"2021\"\n"),
    )
    .expect("manifest should be written");
    for (path, content) in files {
        let path = dir.path().join(path);
        std::fs::create_dir_all(path.parent().unwrap()).expect("source dir should be created");
        std::fs::write(path, content).expect("source should be written");
    }
    dir
}

fn rtc_scan(
    root: &std::path::Path,
    config: polkadot_linter::config::Config,
) -> Vec<polkadot_linter::diagnostics::Diagnostic> {
    let mut engine = polkadot_linter::engine::LintEngine::new(config);
    engine.filter_rules(&["runtime-config".to_string()]);
    engine.scan(root)
}

const RTC_RUNTIME_LIB: &str = r#"
impl pallet_sudo::Config for Runtime {
    type WeightInfo = ();
}

construct_runtime!(
    pub enum Runtime {
        Sudo: pallet_sudo = 15,
    }
);
"#;

#[test]
fn rtc001_detects_a_runtime_built_with_a_forked_construct_runtime() {
    // InvArch declares its runtimes with `construct_runtime_modified!`.
    let fixture = include_str!("fixtures/bad_rtc001.rs");
    let forked = fixture.replace("construct_runtime!", "construct_runtime_modified!");
    let diags = check_fixture("runtime/src/lib.rs", &forked);
    assert_eq!(
        rtc_lines(&diags, "RTC001"),
        vec![7, 12, 16, 20],
        "{diags:?}"
    );
    // Recall guard: the stock macro is unchanged, an unrelated name is not a runtime.
    let diags = check_fixture("runtime/src/lib.rs", fixture);
    assert_eq!(
        rtc_lines(&diags, "RTC001"),
        vec![7, 12, 16, 20],
        "{diags:?}"
    );
    let other = fixture.replace("construct_runtime!", "not_a_runtime!");
    let diags = check_fixture("runtime/src/lib.rs", &other);
    assert!(rtc_lines(&diags, "RTC001").is_empty(), "{diags:?}");
}

#[test]
fn rtc001_detects_placeholder_weight_info() {
    let diags = check_fixture("runtime/src/lib.rs", include_str!("fixtures/bad_rtc001.rs"));
    assert_eq!(
        rtc_lines(&diags, "RTC001"),
        vec![7, 12, 16, 20],
        "{diags:?}"
    );
    assert!(diags
        .iter()
        .filter(|diagnostic| diagnostic.rule_id == "RTC001")
        .all(|diagnostic| diagnostic.severity == polkadot_linter::diagnostics::Severity::Warning));
}

#[test]
fn rtc001_allows_benchmarked_allowlisted_and_test_weights() {
    let diags = check_fixture(
        "runtime/src/lib.rs",
        include_str!("fixtures/good_rtc001.rs"),
    );
    assert!(rtc_findings(&diags).is_empty(), "{diags:?}");
}

#[test]
fn rtc001_reports_substrate_weight_as_advisory_when_enabled() {
    let mut config = polkadot_linter::config::Config::default();
    config.runtime_config.flag_substrate_weight = true;
    let diags = check_fixture_with_config(
        "runtime/src/lib.rs",
        include_str!("fixtures/good_rtc001.rs"),
        &config,
    );
    let findings = diags
        .iter()
        .filter(|diagnostic| diagnostic.rule_id == "RTC001")
        .collect::<Vec<_>>();
    assert_eq!(findings.len(), 1, "{diags:?}");
    assert_eq!(findings[0].line, 19);
    assert_eq!(
        findings[0].severity,
        polkadot_linter::diagnostics::Severity::Advisory
    );
}

#[test]
fn rtc_rules_skip_mock_and_test_paths() {
    let bad = include_str!("fixtures/bad_rtc001.rs");
    for path in [
        "runtime/src/mock.rs",
        "runtime/tests/runtime.rs",
        "runtime/src/mock/mod.rs",
        "runtime/benches/weights.rs",
        "my-test-runtime/src/lib.rs",
    ] {
        let diags = check_fixture(path, bad);
        assert!(rtc_findings(&diags).is_empty(), "{path}: {diags:?}");
    }
}

#[test]
fn rtc_rules_skip_files_outside_runtime_crates() {
    // A pallet crate's mock-like Config impl without a runtime declaration.
    let content = r#"
impl pallet_xcm::Config for Runtime {
    type WeightInfo = pallet_xcm::TestWeightInfo;
    type PriceForSiblingDelivery = ();
}
"#;
    let diags = check_fixture("pallets/foo/src/lib.rs", content);
    assert!(rtc_findings(&diags).is_empty(), "{diags:?}");
}

#[test]
fn rtc002_detects_local_weights_missing_from_define_benchmarks() {
    let diags = check_fixture("runtime/src/lib.rs", include_str!("fixtures/bad_rtc002.rs"));
    assert_eq!(rtc_lines(&diags, "RTC002"), vec![10, 14, 20], "{diags:?}");
}

#[test]
fn rtc002_accepts_benchmarked_pallets() {
    let diags = check_fixture(
        "runtime/src/lib.rs",
        include_str!("fixtures/good_rtc002.rs"),
    );
    assert!(rtc_findings(&diags).is_empty(), "{diags:?}");
}

#[test]
fn rtc002_reads_define_benchmarks_from_sibling_files_and_requires_it() {
    let configs = r#"
impl pallet_balances::Config for Runtime {
    type WeightInfo = weights::pallet_balances::WeightInfo<Runtime>;
}
impl pallet_sudo::Config for Runtime {
    type WeightInfo = weights::pallet_sudo::WeightInfo<Runtime>;
}
"#;
    let lib = "mod configs;\nmod benchmarks;\nconstruct_runtime!(pub enum Runtime { Balances: pallet_balances = 10 });\n";
    let benches = "frame_benchmarking::define_benchmarks!(\n    [pallet_balances, Balances]\n);\n";
    let krate = rtc_runtime_crate(
        "example-runtime",
        &[
            ("src/lib.rs", lib),
            ("src/configs/mod.rs", configs),
            ("src/benchmarks.rs", benches),
        ],
    );
    let diags = rtc_scan(krate.path(), polkadot_linter::config::Config::default());
    assert_eq!(
        rtc_findings(&diags),
        vec!["RTC002:6".to_string()],
        "{diags:?}"
    );

    // Without any define_benchmarks! the precondition is not met.
    let krate = rtc_runtime_crate(
        "example-runtime",
        &[("src/lib.rs", lib), ("src/configs/mod.rs", configs)],
    );
    let diags = rtc_scan(krate.path(), polkadot_linter::config::Config::default());
    assert!(rtc_findings(&diags).is_empty(), "{diags:?}");
}

#[test]
fn rtc003_detects_waived_xcm_fees() {
    let diags = check_fixture(
        "runtime/src/xcm_config.rs",
        include_str!("fixtures/bad_rtc003.rs"),
    );
    assert_eq!(
        rtc_lines(&diags, "RTC003"),
        vec![6, 13, 17, 20, 28, 39],
        "{diags:?}"
    );
    assert!(rtc_lines(&diags, "RTC006").is_empty(), "{diags:?}");
    assert!(diags
        .iter()
        .filter(|diagnostic| diagnostic.rule_id == "RTC003")
        .all(|diagnostic| diagnostic.severity == polkadot_linter::diagnostics::Severity::Error));
}

#[test]
fn rtc003_allows_fee_managers_exponential_prices_and_senderless_executors() {
    let diags = check_fixture(
        "runtime/src/xcm_config.rs",
        include_str!("fixtures/good_rtc003.rs"),
    );
    assert!(rtc_findings(&diags).is_empty(), "{diags:?}");
}

#[test]
fn rtc004_detects_zero_deposits() {
    let diags = check_fixture("runtime/src/lib.rs", include_str!("fixtures/bad_rtc004.rs"));
    assert_eq!(
        rtc_lines(&diags, "RTC004"),
        vec![4, 7, 8, 9, 15, 16, 20, 24, 28, 29, 32, 38, 50],
        "{diags:?}"
    );
}

#[test]
fn rtc004_allows_non_zero_gated_and_benchmark_only_deposits() {
    let diags = check_fixture(
        "runtime/src/lib.rs",
        include_str!("fixtures/good_rtc004.rs"),
    );
    assert!(rtc_findings(&diags).is_empty(), "{diags:?}");
}

#[test]
fn rtc005_detects_collective_flip_randomness() {
    let diags = check_fixture("runtime/src/lib.rs", include_str!("fixtures/bad_rtc005.rs"));
    assert_eq!(rtc_lines(&diags, "RTC005"), vec![10, 16, 17], "{diags:?}");
}

#[test]
fn rtc005_allows_vrf_randomness_and_storage_prefix_bytes() {
    let diags = check_fixture(
        "runtime/src/lib.rs",
        include_str!("fixtures/good_rtc005.rs"),
    );
    assert!(rtc_findings(&diags).is_empty(), "{diags:?}");
}

#[test]
fn rtc_rules_skip_dev_runtimes_unless_opted_in() {
    let krate = rtc_runtime_crate("kitchensink-runtime", &[("src/lib.rs", RTC_RUNTIME_LIB)]);
    let diags = rtc_scan(krate.path(), polkadot_linter::config::Config::default());
    assert!(rtc_findings(&diags).is_empty(), "{diags:?}");

    let mut config = polkadot_linter::config::Config::default();
    config.runtime_config.include_dev_runtimes = true;
    let diags = rtc_scan(krate.path(), config);
    assert_eq!(
        rtc_findings(&diags),
        vec!["RTC001:3".to_string()],
        "{diags:?}"
    );
}

#[test]
fn rtc_rules_skip_dev_runtimes_by_whole_name_token() {
    for package in ["peaq-dev", "development-runtime"] {
        let krate = rtc_runtime_crate(package, &[("src/lib.rs", RTC_RUNTIME_LIB)]);
        let diags = rtc_scan(krate.path(), polkadot_linter::config::Config::default());
        assert!(rtc_findings(&diags).is_empty(), "{package}: {diags:?}");
        let mut config = polkadot_linter::config::Config::default();
        config.runtime_config.include_dev_runtimes = true;
        let diags = rtc_scan(krate.path(), config);
        assert_eq!(
            rtc_findings(&diags),
            vec!["RTC001:3".to_string()],
            "{package}"
        );
    }
    for package in ["devops-x-runtime", "devnet-runtime"] {
        let krate = rtc_runtime_crate(package, &[("src/lib.rs", RTC_RUNTIME_LIB)]);
        let diags = rtc_scan(krate.path(), polkadot_linter::config::Config::default());
        assert_eq!(
            rtc_findings(&diags),
            vec!["RTC001:3".to_string()],
            "{package}"
        );
    }
}

#[test]
fn rtc_checks_unsaved_file_against_its_crate() {
    // An editor buffer not yet on disk still belongs to the enclosing crate.
    let dir = tempfile::tempdir().expect("tempdir");
    let root = dir.path();
    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"review-runtime\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
    )
    .unwrap();
    std::fs::write(
        root.join("src/lib.rs"),
        "construct_runtime!(pub enum Runtime { Sudo: pallet_sudo = 15 });",
    )
    .unwrap();
    let path = root.join("src/new_config.rs");
    assert!(!path.exists());
    let diags = check_fixture_path(
        path,
        "impl pallet_sudo::Config for Runtime { type WeightInfo = (); }",
    );
    assert_eq!(rtc_lines(&diags, "RTC001"), vec![1], "{diags:?}");
}

#[test]
fn findings_ignore_test_named_ancestors_above_scan_root() {
    let sem = format!("use foo::*;\n{}", include_str!("fixtures/bad_sem003.rs"));
    let dir = tempfile::tempdir().expect("temporary parent should be created");
    let scan = |ancestor: &str| {
        let root = dir.path().join(ancestor).join("westend-runtime");
        std::fs::create_dir_all(root.join("src")).expect("source dir should be created");
        std::fs::write(
            root.join("Cargo.toml"),
            "[package]\nname = \"westend-runtime\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
        )
        .expect("manifest should be written");
        std::fs::write(root.join("src/lib.rs"), RTC_RUNTIME_LIB).expect("lib should be written");
        std::fs::write(root.join("src/sem.rs"), &sem).expect("source should be written");
        let engine = polkadot_linter::engine::LintEngine::new(all_rules_config());
        let mut findings = engine
            .scan(&root)
            .iter()
            .map(|d| {
                format!(
                    "{}:{}:{}",
                    d.rule_id,
                    d.file.strip_prefix(&root).unwrap().display(),
                    d.line
                )
            })
            .collect::<Vec<_>>();
        findings.sort();
        findings
    };
    let baseline = scan("ok");
    for rule in ["RTC001", "SEM003", "SEM004"] {
        assert!(
            baseline.iter().any(|f| f.starts_with(rule)),
            "{rule}: {baseline:?}"
        );
    }
    for ancestor in ["test", "tests"] {
        assert_eq!(scan(ancestor), baseline, "scan root under {ancestor}/");
    }
}

#[test]
fn narrow_scans_classify_test_and_bench_files_from_the_crate_root() {
    let dir = tempfile::tempdir().expect("temporary crate should be created");
    let krate = dir.path().join("c");
    let helper = "use std::collections::*;\npub fn check() {\n    let v: Vec<u32> = vec![1, 2];\n    for x in v.iter() { let _ = x; }\n    let r: Result<u32, ()> = Err(());\n    assert!(r.is_err());\n    let _ = r.unwrap_err();\n}\n";
    for (file, content) in [
        (
            "Cargo.toml",
            "[package]\nname = \"c\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
        ),
        ("src/lib.rs", "pub fn f() {}\n"),
        (
            "tests/main.rs",
            "mod support;\n#[test]\nfn t() { support::check(); }\n",
        ),
        ("tests/support/mod.rs", helper),
        ("benches/benchmarks/helper.rs", helper),
    ] {
        let path = krate.join(file);
        std::fs::create_dir_all(path.parent().unwrap()).expect("dir should be created");
        std::fs::write(path, content).expect("file should be written");
    }
    let engine = polkadot_linter::engine::LintEngine::new(all_rules_config());
    let rules = |target: &str| {
        let mut ids = engine
            .scan(&krate.join(target))
            .into_iter()
            .map(|d| d.rule_id)
            .filter(|id| ["TST001", "SEM003", "SEM004"].contains(&id.as_str()))
            .collect::<Vec<_>>();
        ids.sort();
        ids
    };
    assert_eq!(rules("tests"), ["TST001"]);
    assert_eq!(rules("tests/support/mod.rs"), ["TST001"]);
    assert_eq!(rules("benches/benchmarks"), ["SEM003"]);
    assert_eq!(rules(""), ["SEM003", "TST001"]);
}

#[test]
fn rtc_rules_always_skip_test_runtime_packages() {
    let krate = rtc_runtime_crate("polkadot-test-runtime", &[("src/lib.rs", RTC_RUNTIME_LIB)]);
    let mut config = polkadot_linter::config::Config::default();
    config.runtime_config.include_dev_runtimes = true;
    let diags = rtc_scan(krate.path(), config);
    assert!(rtc_findings(&diags).is_empty(), "{diags:?}");
}

#[test]
fn rtc_rules_lint_production_runtime_crates_only() {
    let krate = rtc_runtime_crate("westend-runtime", &[("src/lib.rs", RTC_RUNTIME_LIB)]);
    let diags = rtc_scan(krate.path(), polkadot_linter::config::Config::default());
    assert_eq!(
        rtc_findings(&diags),
        vec!["RTC001:3".to_string()],
        "{diags:?}"
    );

    // A source file under a virtual workspace manifest is not a runtime crate.
    let dir = tempfile::tempdir().expect("temporary workspace should be created");
    std::fs::write(dir.path().join("Cargo.toml"), "[workspace]\nmembers = []\n")
        .expect("manifest should be written");
    std::fs::create_dir_all(dir.path().join("orphan/src")).expect("orphan dir");
    std::fs::write(dir.path().join("orphan/src/lib.rs"), RTC_RUNTIME_LIB).expect("orphan source");
    let diags = rtc_scan(dir.path(), polkadot_linter::config::Config::default());
    assert!(rtc_findings(&diags).is_empty(), "{diags:?}");
}

#[test]
fn rtc006_reports_burned_xcm_fees_as_warning() {
    let diags = check_fixture(
        "runtime/src/xcm_config.rs",
        include_str!("fixtures/bad_rtc006.rs"),
    );
    assert_eq!(rtc_lines(&diags, "RTC006"), vec![8], "{diags:?}");
    // One free price among priced ones does not make delivery free.
    assert_eq!(rtc_lines(&diags, "RTC003"), vec![26], "{diags:?}");
    assert!(diags
        .iter()
        .filter(|diagnostic| diagnostic.rule_id == "RTC006")
        .all(|diagnostic| diagnostic.severity == polkadot_linter::diagnostics::Severity::Warning));
}

#[test]
fn rtc006_ignores_unit_fee_managers_when_every_delivery_price_is_free() {
    let diags = check_fixture(
        "runtime/src/xcm_config.rs",
        include_str!("fixtures/good_rtc006.rs"),
    );
    assert!(rtc_lines(&diags, "RTC006").is_empty(), "{diags:?}");
    assert_eq!(rtc_lines(&diags, "RTC003"), vec![6, 18], "{diags:?}");
}

fn rtc009_lockfile(packages: &[(&str, &str, Option<&str>)]) -> String {
    let mut lock = "version = 4\n".to_string();
    for (name, version, source) in packages {
        lock += &format!("\n[[package]]\nname = \"{name}\"\nversion = \"{version}\"\n");
        if let Some(source) = source {
            lock += &format!("source = \"{source}\"\n");
        }
    }
    lock
}

const CRATES_IO: Option<&str> = Some("registry+https://github.com/rust-lang/crates.io-index");

#[test]
fn sem019_reports_verifier_trait_defaults_that_accept_everything() {
    let diags = check_fixture(
        "substrate/primitives/runtime/src/traits/mod.rs",
        include_str!("fixtures/bad_sem019.rs"),
    );
    assert_eq!(rtc_lines(&diags, "SEM019"), vec![6, 10], "{diags:?}");
}

#[test]
fn rtc009_reports_vulnerable_xcm_executor_releases_in_cargo_lock() {
    // fellows runtimes before 2.5.1 (28.0.1) and before the Feb 2026 bump (23.0.0, 21.0.0).
    for (version, expected) in [
        ("28.0.1", "upgrade to 28.1.0 or later"),
        ("25.0.0", "upgrade to 25.2.0 or later"),
        ("23.0.0", "upgrade to 23.0.1 or later"),
        ("19.1.3", "upgrade to 19.1.4 or later"),
        (
            "21.0.0",
            "the 21.x line has no fixed release; the nearest fixed release is 22.0.1",
        ),
        (
            "26.0.0",
            "the 26.x line has no fixed release; the nearest fixed release is 27.1.0",
        ),
        (
            "18.0.1",
            "the 18.x line has no fixed release; the nearest fixed release is 19.1.4",
        ),
    ] {
        let lock = rtc009_lockfile(&[
            ("serde", "1.0.0", CRATES_IO),
            ("staging-xcm-executor", version, CRATES_IO),
        ]);
        let diags = check_fixture("Cargo.lock", &lock);
        assert_eq!(rtc_lines(&diags, "RTC009"), vec![9], "{version}: {diags:?}");
        let message = &diags
            .iter()
            .find(|d| d.rule_id == "RTC009")
            .unwrap()
            .message;
        assert!(
            message.contains(&format!("staging-xcm-executor {version} ")),
            "{message}"
        );
        assert!(message.contains(expected), "{message}");
        assert!(message.contains("https://"), "{message}");
    }
}

#[test]
fn rtc009_ignores_fixed_unaffected_and_local_xcm_executor_versions() {
    for (version, source) in [
        ("28.1.0", CRATES_IO), // first fixed (runtimes 2.5.1)
        ("24.0.1", CRATES_IO), // InitiateTransfer fix
        ("29.0.0", CRATES_IO), // released with the dotUSD fix
        ("18.0.0", CRATES_IO), // before the InitiateTransfer regression
        ("19.0.0", CRATES_IO), // yanked, without the regression
        ("25.0.0", None),      // workspace path package (polkadot-sdk itself)
        (
            "25.0.0",
            Some("git+https://github.com/paritytech/polkadot-sdk?branch=master#0"),
        ),
        ("28.0.1-rc1", CRATES_IO),
        // A private registry may serve a patched fork with the same version.
        ("28.0.1", Some("registry+https://crates.example.com/index")),
        ("28.0.1", Some("sparse+https://crates.example.com/index/")),
    ] {
        let lock = rtc009_lockfile(&[("staging-xcm-executor", version, source)]);
        let diags = check_fixture("Cargo.lock", &lock);
        assert!(
            rtc_lines(&diags, "RTC009").is_empty(),
            "{version}: {diags:?}"
        );
    }
    // Only `Cargo.lock` files are lockfiles.
    let lock = rtc009_lockfile(&[("staging-xcm-executor", "28.0.1", CRATES_IO)]);
    assert!(rtc_lines(&check_fixture("deps.toml", &lock), "RTC009").is_empty());
}

#[test]
fn rtc009_reports_the_name_line_of_the_matching_package_block() {
    let lock = "version = 4\n\n[[package]]\nname = \"staging-xcm-executor\"\nversion = \"29.0.0\"\n\
source = \"registry+https://github.com/rust-lang/crates.io-index\"\n\n[[package]]\n\
version = \"28.0.1\"\nsource = \"sparse+https://index.crates.io/\"\nname = \"staging-xcm-executor\"\n";
    let diags = check_fixture("Cargo.lock", lock);
    assert_eq!(rtc_lines(&diags, "RTC009"), vec![11], "{diags:?}");
}

#[test]
fn rtc009_runs_on_cargo_lock_in_a_directory_scan() {
    let dir = tempfile::tempdir().expect("temporary workspace should be created");
    let lock = rtc009_lockfile(&[("staging-xcm-executor", "28.0.1", CRATES_IO)]);
    std::fs::write(dir.path().join("Cargo.lock"), lock).expect("lockfile should be written");
    let diags = rtc_scan(dir.path(), polkadot_linter::config::Config::default());
    assert_eq!(rtc_lines(&diags, "RTC009"), vec![4], "{diags:?}");
}

#[test]
fn rtc007_detects_non_recursive_instruction_denies() {
    let diags = check_fixture(
        "runtime/src/xcm_config.rs",
        include_str!("fixtures/bad_rtc007.rs"),
    );
    assert_eq!(
        rtc_lines(&diags, "RTC007"),
        vec![7, 13, 13, 17],
        "{diags:?}"
    );
    let good = check_fixture(
        "runtime/src/xcm_config.rs",
        include_str!("fixtures/good_rtc007.rs"),
    );
    assert!(rtc_findings(&good).is_empty(), "{good:?}");
}

#[test]
fn rtc008_detects_fail_open_proxy_filters() {
    let diags = check_fixture("runtime/src/lib.rs", include_str!("fixtures/bad_rtc008.rs"));
    assert_eq!(rtc_lines(&diags, "RTC008"), vec![20, 60], "{diags:?}");
    let messages: Vec<_> = diags.iter().map(|d| d.message.as_str()).collect();
    assert!(
        messages[0].contains("PolkadotXcm (pallet_xcm), ForeignAssets (pallet_assets)"),
        "{messages:?}"
    );
    assert!(
        messages[1].contains(
            "`NonTransfer` can add `Collator` proxies, which admit `CollatorSelection` calls \
             `NonTransfer` does not"
        ),
        "{messages:?}"
    );
    let good = check_fixture(
        "runtime/src/lib.rs",
        include_str!("fixtures/good_rtc008.rs"),
    );
    assert!(rtc_findings(&good).is_empty(), "{good:?}");
}

#[test]
fn rtc010_reports_authorized_aliasers_on_the_unpaid_barrier() {
    let diags = check_fixture(
        "runtime/src/xcm_config.rs",
        include_str!("fixtures/bad_rtc010.rs"),
    );
    assert_eq!(rtc_lines(&diags, "RTC010"), vec![22, 26, 27], "{diags:?}");
    // Outside a runtime crate the same barrier is not reported.
    let diags = check_fixture(
        "pallets/xcm-mock/src/lib.rs",
        &include_str!("fixtures/bad_rtc010.rs").replace("construct_runtime!", "not_a_runtime!"),
    );
    assert!(rtc_lines(&diags, "RTC010").is_empty(), "{diags:?}");
}

#[test]
fn rtc010_reports_an_ambiguous_alias_only_if_every_definition_reads_storage() {
    let xcm = |second: &str| {
        format!(
            "construct_runtime!(pub enum Runtime {{ PolkadotXcm: pallet_xcm = 31 }});\n\
mod kusama {{\n    pub type Aliasers = (AliasChildLocation, AuthorizedAliasers<Runtime>);\n}}\n\
mod polkadot {{\n    pub type Aliasers = {second};\n}}\n\
pub type Barrier = AllowExplicitUnpaidExecutionFrom<Equals<Parent>, Aliasers>;\n"
        )
    };
    let diags = check_fixture("runtime/src/xcm_config.rs", &xcm("(AliasChildLocation,)"));
    assert!(rtc_lines(&diags, "RTC010").is_empty(), "{diags:?}");
    let diags = check_fixture(
        "runtime/src/xcm_config.rs",
        &xcm("AuthorizedAliasers<Runtime>"),
    );
    assert_eq!(rtc_lines(&diags, "RTC010"), vec![8], "{diags:?}");
}

#[test]
fn rtc003_stays_error_with_a_minimal_user_config() {
    let config: polkadot_linter::config::Config =
        toml::from_str("[rules]\n").expect("minimal user configuration must deserialize");
    let diags = check_fixture_with_config(
        "runtime/src/xcm_config.rs",
        include_str!("fixtures/bad_rtc003.rs"),
        &config,
    );
    let severities = diags
        .iter()
        .filter(|diagnostic| diagnostic.rule_id.starts_with("RTC"))
        .map(|diagnostic| (diagnostic.rule_id.as_str(), diagnostic.severity))
        .collect::<Vec<_>>();
    assert!(!severities.is_empty(), "{diags:?}");
    assert!(
        severities
            .iter()
            .all(|(_, severity)| *severity == polkadot_linter::diagnostics::Severity::Error),
        "{severities:?}"
    );
}

#[test]
fn rtc_rules_recognise_a_bare_runtime_attribute() {
    let lib = r#"
use frame_support::runtime;

impl pallet_sudo::Config for Runtime {
    type WeightInfo = ();
}

#[runtime]
mod runtime {
    #[runtime::runtime]
    #[runtime::derive(RuntimeCall, RuntimeEvent)]
    pub struct Runtime;

    #[runtime::pallet_index(0)]
    pub type Sudo = pallet_sudo;
}
"#;
    let krate = rtc_runtime_crate("bare-attr-runtime", &[("src/lib.rs", lib)]);
    let diags = rtc_scan(krate.path(), polkadot_linter::config::Config::default());
    assert_eq!(
        rtc_findings(&diags),
        vec!["RTC001:5".to_string()],
        "{diags:?}"
    );

    // A bare `#[runtime]` on a module without `#[runtime::*]` items is not a
    // runtime declaration.
    let lib = "#[runtime]\nmod runtime { pub struct Runtime; }\nimpl pallet_sudo::Config for Runtime {\n    type WeightInfo = ();\n}\n";
    let krate = rtc_runtime_crate("not-a-runtime", &[("src/lib.rs", lib)]);
    let diags = rtc_scan(krate.path(), polkadot_linter::config::Config::default());
    assert!(rtc_findings(&diags).is_empty(), "{diags:?}");
}

#[test]
fn rtc_rules_follow_lib_path_and_reachable_modules() {
    // construct_runtime! lives in a module re-exported from lib.rs.
    let krate = rtc_runtime_crate(
        "module-runtime",
        &[
            ("src/lib.rs", "mod runtime;\npub use runtime::*;\n"),
            ("src/runtime.rs", RTC_RUNTIME_LIB),
        ],
    );
    let diags = rtc_scan(krate.path(), polkadot_linter::config::Config::default());
    assert_eq!(
        rtc_findings(&diags),
        vec!["RTC001:3".to_string()],
        "{diags:?}"
    );

    // `[lib] path` points at a non-default library root.
    let dir = tempfile::tempdir().expect("temporary runtime crate should be created");
    std::fs::write(
        dir.path().join("Cargo.toml"),
        "[package]\nname = \"custom-lib-runtime\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n[lib]\npath = \"runtime/lib.rs\"\n",
    )
    .expect("manifest should be written");
    std::fs::create_dir_all(dir.path().join("runtime")).expect("lib dir");
    std::fs::write(dir.path().join("runtime/lib.rs"), RTC_RUNTIME_LIB).expect("lib source");
    let diags = rtc_scan(dir.path(), polkadot_linter::config::Config::default());
    assert_eq!(
        rtc_findings(&diags),
        vec!["RTC001:3".to_string()],
        "{diags:?}"
    );
}

#[test]
fn rtc_rules_match_test_package_names_by_whole_token() {
    for package in [
        "mychain-testnet-runtime",
        "contest-runtime",
        "mockingbird-runtime",
    ] {
        let krate = rtc_runtime_crate(package, &[("src/lib.rs", RTC_RUNTIME_LIB)]);
        let diags = rtc_scan(krate.path(), polkadot_linter::config::Config::default());
        assert_eq!(
            rtc_findings(&diags),
            vec!["RTC001:3".to_string()],
            "{package}: {diags:?}"
        );
    }

    let krate = rtc_runtime_crate("xcm_mock_runtime", &[("src/lib.rs", RTC_RUNTIME_LIB)]);
    let diags = rtc_scan(krate.path(), polkadot_linter::config::Config::default());
    assert!(rtc_findings(&diags).is_empty(), "{diags:?}");

    // The token list is configurable: an empty list lints test runtimes too.
    let krate = rtc_runtime_crate("polkadot-test-runtime", &[("src/lib.rs", RTC_RUNTIME_LIB)]);
    let mut config = polkadot_linter::config::Config::default();
    config.runtime_config.test_runtime_tokens.clear();
    let diags = rtc_scan(krate.path(), config);
    assert_eq!(
        rtc_findings(&diags),
        vec!["RTC001:3".to_string()],
        "{diags:?}"
    );
}

#[test]
fn rtc002_ignores_define_benchmarks_in_test_modules() {
    let lib = r#"
impl pallet_sudo::Config for Runtime {
    type WeightInfo = weights::pallet_sudo::WeightInfo<Runtime>;
}

construct_runtime!(pub enum Runtime { Sudo: pallet_sudo = 15 });

#[cfg(feature = "runtime-benchmarks")]
mod benches {
    frame_benchmarking::define_benchmarks!([pallet_balances, Balances]);
}

#[cfg(test)]
mod tests {
    frame_benchmarking::define_benchmarks!([pallet_sudo, Sudo]);
}
"#;
    let krate = rtc_runtime_crate("bench-runtime", &[("src/lib.rs", lib)]);
    let diags = rtc_scan(krate.path(), polkadot_linter::config::Config::default());
    assert_eq!(
        rtc_findings(&diags),
        vec!["RTC002:3".to_string()],
        "{diags:?}"
    );
}

#[test]
fn rtc_rules_skip_files_behind_test_or_benchmark_module_declarations() {
    let placeholder = "impl pallet_sudo::Config for Runtime {\n    type WeightInfo = ();\n}\n";
    let lib = "#[cfg(test)]\nmod sandbox;\n#[cfg(feature = \"runtime-benchmarks\")]\nmod bench_config;\nmod configs;\nconstruct_runtime!(pub enum Runtime { Sudo: pallet_sudo = 15 });\n";
    let krate = rtc_runtime_crate(
        "sandbox-runtime",
        &[
            ("src/lib.rs", lib),
            ("src/sandbox.rs", placeholder),
            ("src/bench_config.rs", placeholder),
            ("src/configs/mod.rs", placeholder),
        ],
    );
    let diags = rtc_scan(krate.path(), polkadot_linter::config::Config::default());
    let findings = diags
        .iter()
        .filter(|diagnostic| diagnostic.rule_id.starts_with("RTC"))
        .map(|diagnostic| {
            diagnostic
                .file
                .strip_prefix(krate.path())
                .unwrap_or(&diagnostic.file)
                .to_string_lossy()
                .replace('\\', "/")
        })
        .collect::<Vec<_>>();
    assert_eq!(
        findings,
        vec!["src/configs/mod.rs".to_string()],
        "{diags:?}"
    );
}

#[test]
fn rtc_rules_lint_runtimes_switched_by_feature() {
    let decl =
        "construct_runtime!(\n    pub enum Runtime {\n        Sudo: pallet_sudo = 15,\n    }\n);\n";
    let config = "impl pallet_sudo::Config for Runtime {\n    type WeightInfo = ();\n}\n";
    // Both declarations gated by `feature` (directly, inside `not`, or via
    // an enclosing module or file): the crate is still a runtime.
    for lib in [
        format!("{config}#[cfg(feature = \"with-evm\")]\n{decl}#[cfg(not(feature = \"with-evm\"))]\n{decl}"),
        format!("{config}#[cfg(any(feature = \"a\", test))]\nmod a {{\n{decl}}}\n#[cfg(not(any(feature = \"a\", test)))]\nmod b {{\n{decl}}}\n"),
        format!("{config}#[cfg(feature = \"a\")]\nmod a;\n#[cfg(not(feature = \"a\"))]\nmod b;\n"),
    ] {
        let krate = rtc_runtime_crate(
            "switched-runtime",
            &[
                ("src/lib.rs", lib.as_str()),
                ("src/a.rs", decl),
                ("src/b.rs", &format!("#![cfg(feature = \"x\")]\n{decl}")),
            ],
        );
        let diags = rtc_scan(krate.path(), polkadot_linter::config::Config::default());
        assert_eq!(rtc_findings(&diags), vec!["RTC001:2".to_string()], "{lib}\n{diags:?}");
    }

    // Two ungated runtimes (an XCM simulator) stay excluded, even next to a
    // feature-gated one.
    let lib =
        format!("{config}mod a {{\n{decl}}}\nmod b {{\n{decl}}}\n#[cfg(feature = \"c\")]\n{decl}");
    let krate = rtc_runtime_crate("simulator", &[("src/lib.rs", lib.as_str())]);
    let diags = rtc_scan(krate.path(), polkadot_linter::config::Config::default());
    assert!(rtc_findings(&diags).is_empty(), "{diags:?}");
}

#[test]
fn rtc002_matches_benchmark_paths_by_whole_segments() {
    let lib = "construct_runtime!(pub enum Runtime { Balances: pallet_balances = 10 });\n\
impl custom_pallet_balances::Config for Runtime {\n    type WeightInfo = weights::custom_pallet_balances::WeightInfo<Runtime>;\n}\n\
impl parachains_paras::Config for Runtime {\n    type WeightInfo = weights::polkadot_runtime_parachains_paras::WeightInfo<Runtime>;\n}\n\
impl disputes::Config for Runtime {\n    type WeightInfo = weights::disputes::WeightInfo<Runtime>;\n}\n\
define_benchmarks!(\n    [pallet_balances, Balances]\n    [polkadot_runtime_parachains::paras, Paras]\n    [polkadot_runtime_parachains::disputes, Disputes]\n);\n";
    let krate = rtc_runtime_crate("segment-runtime", &[("src/lib.rs", lib)]);
    let diags = rtc_scan(krate.path(), polkadot_linter::config::Config::default());
    assert_eq!(rtc_lines(&diags, "RTC002"), vec![3], "{diags:?}");
}

#[test]
fn rtc_rules_treat_aliases_redefined_in_nested_modules_as_ambiguous() {
    // RTC003: the outer waiver is `Everything`; a nested `Nothing` must not
    // hide it.
    let xcm = "construct_runtime!(pub enum Runtime { System: frame_system = 0 });\n\
type WaivedLocations = Everything;\n\
impl xcm_executor::Config for XcmConfig {\n    type FeeManager = XcmFeeManagerFromComponents<WaivedLocations, ()>;\n}\n\
mod nested {\n    type WaivedLocations = Nothing;\n}\n";
    let diags = check_fixture("runtime/src/xcm_config.rs", xcm);
    assert_eq!(rtc_lines(&diags, "RTC003"), vec![4], "{diags:?}");

    // RTC004: the outer origin is signed; a nested `EnsureRoot` must not
    // exempt the zero deposit.
    let lib = "construct_runtime!(pub enum Runtime { Referenda: pallet_referenda = 1 });\n\
type SubmissionOrigin = EnsureSigned<AccountId>;\n\
impl pallet_referenda::Config for Runtime {\n    type SubmitOrigin = SubmissionOrigin;\n    type SubmissionDeposit = ConstU128<0>;\n}\n\
mod nested {\n    type SubmissionOrigin = EnsureRoot<AccountId>;\n}\n";
    let diags = check_fixture("runtime/src/lib.rs", lib);
    assert_eq!(rtc_lines(&diags, "RTC004"), vec![5], "{diags:?}");
}

// ==========================================================================
// Noise narrowing (round 2): one negative case per mechanism, plus recall guards
// ==========================================================================

/// Names (first backticked word of the message) reported for `rule_id`, sorted.
fn reported_names(
    diags: &[polkadot_linter::diagnostics::Diagnostic],
    rule_id: &str,
) -> Vec<String> {
    let mut names = diags
        .iter()
        .filter(|d| d.rule_id == rule_id)
        .map(|d| d.message.split('`').nth(1).unwrap().to_string())
        .collect::<Vec<_>>();
    names.sort();
    names
}

/// Writes `files` (relative path, content) under a fresh temp dir.
fn write_tree(files: &[(&str, &str)]) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    for (path, content) in files {
        let path = dir.path().join(path);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, content).unwrap();
    }
    dir
}

#[test]
fn ben003_skips_privileged_hand_weighted_reused_and_gas_weighted_calls() {
    let lib = r#"
#[pallet::call]
impl<T: Config> Pallet<T> {
    // #1: privileged origin and a hand-written DbWeight weight.
    #[pallet::call_index(0)]
    #[pallet::weight(T::DbWeight::get().writes(1))]
    pub fn root_set(origin: OriginFor<T>) -> DispatchResult { ensure_root(origin)?; Ok(()) }
    // Recall: privileged, but its WeightInfo fn has no benchmark.
    #[pallet::call_index(1)]
    #[pallet::weight(T::WeightInfo::governance_set())]
    pub fn governance_set(origin: OriginFor<T>) -> DispatchResult {
        T::AdminOrigin::ensure_origin(origin)?;
        Ok(())
    }
    // Recall: signed with a hand-written weight.
    #[pallet::call_index(2)]
    #[pallet::weight(T::DbWeight::get().writes(1))]
    pub fn signed_set(origin: OriginFor<T>) -> DispatchResult { ensure_signed(origin)?; Ok(()) }
    // Recall: `ensure_none` is unsigned, not privileged.
    #[pallet::call_index(3)]
    #[pallet::weight(Weight::from_parts(10_000, 0))]
    pub fn report(origin: OriginFor<T>) -> DispatchResult { ensure_none(origin)?; Ok(()) }
    // #5: reuses the benchmarked `submit` weight.
    #[pallet::call_index(4)]
    #[pallet::weight(T::WeightInfo::submit())]
    pub fn submit_with_limit(origin: OriginFor<T>) -> DispatchResult { ensure_signed(origin)?; Ok(()) }
    // Recall: privileged and hand-weighted, but loops or calls a helper.
    #[pallet::call_index(6)]
    #[pallet::weight(T::DbWeight::get().writes(1))]
    pub fn root_loop(origin: OriginFor<T>, n: u32) -> DispatchResult {
        ensure_root(origin)?;
        for i in 0..n { Value::<T>::insert(i, i); }
        Ok(())
    }
    #[pallet::call_index(7)]
    #[pallet::weight(T::DbWeight::get().writes(1))]
    pub fn root_helper(origin: OriginFor<T>) -> DispatchResult {
        T::ForceOrigin::ensure_origin(origin)?;
        Self::do_fallback()
    }
    #[pallet::call_index(8)]
    #[pallet::weight(T::DbWeight::get().writes(1))]
    pub fn root_iter(origin: OriginFor<T>) -> DispatchResult {
        ensure_root(origin)?;
        let _ = Value::<T>::iter_keys().count();
        Ok(())
    }
    // Recall: `gas` only as part of a longer token.
    #[pallet::call_index(9)]
    #[pallet::weight(T::WeightInfo::pegasus())]
    pub fn pegasus(origin: OriginFor<T>) -> DispatchResult { ensure_signed(origin)?; Ok(()) }
    // #10: priced by gas.
    #[pallet::call_index(5)]
    #[pallet::weight(T::GasWeightMapping::gas_to_weight(*gas_limit, true))]
    pub fn call(origin: OriginFor<T>, gas_limit: u64) -> DispatchResult { ensure_signed(origin)?; Ok(()) }
}
"#;
    let bench = r#"
#[benchmark]
fn submit() {
    #[extrinsic_call]
    submit(RawOrigin::Signed(caller));
}
"#;
    let dir = write_tree(&[
        ("pallets/foo/src/lib.rs", lib),
        ("pallets/foo/src/benchmarking.rs", bench),
    ]);
    let diags = check_fixture_path(dir.path().join("pallets/foo/src/lib.rs"), lib);
    assert_eq!(
        reported_names(&diags, "BEN003"),
        vec![
            "governance_set",
            "pegasus",
            "report",
            "root_helper",
            "root_iter",
            "root_loop",
            "signed_set"
        ],
        "{diags:?}"
    );
}

#[test]
fn ben003_and_tst006_skip_crates_under_test_runtime_or_tests_only() {
    let lib = r#"
#[pallet::call]
impl<T: Config> Pallet<T> {
    #[pallet::call_index(0)]
    pub fn store(origin: OriginFor<T>, v: u32) -> DispatchResult {
        ensure_signed(origin)?;
        Value::<T>::put(v);
        Ok(())
    }
}
"#;
    let manifest = |name: &str| format!("[package]\nname = \"{name}\"\nversion = \"0.1.0\"\n");
    let (test_runtime, example) = (manifest("test-runtime"), manifest("pallet-example"));
    let dir = write_tree(&[
        ("Cargo.toml", "[workspace]\nmembers = []\n"),
        ("runtime/test-runtime/Cargo.toml", &test_runtime),
        ("runtime/test-runtime/src/lib.rs", lib),
        // Recall: examples and templates are forked into production.
        ("examples/basic/Cargo.toml", &example),
        ("examples/basic/src/lib.rs", lib),
    ]);
    let skipped = check_fixture_path(dir.path().join("runtime/test-runtime/src/lib.rs"), lib);
    assert!(
        !has_rule(&skipped, "BEN003") && !has_rule(&skipped, "TST006"),
        "{skipped:?}"
    );
    let kept = check_fixture_path(dir.path().join("examples/basic/src/lib.rs"), lib);
    assert!(
        has_rule(&kept, "BEN003") && has_rule(&kept, "TST006"),
        "{kept:?}"
    );
}

#[test]
fn tst006_skips_privileged_single_setters_and_transient_storage() {
    let code = r#"
#[pallet::call]
impl<T: Config> Pallet<T> {
    // #2: privileged single-item setter.
    #[pallet::call_index(0)]
    pub fn set_fee(origin: OriginFor<T>, fee: u32) -> DispatchResult {
        ensure_root(origin)?;
        Fee::<T>::put(fee);
        Ok(())
    }
    // Recall: signed single setter.
    #[pallet::call_index(1)]
    pub fn set_own_fee(origin: OriginFor<T>, fee: u32) -> DispatchResult {
        ensure_signed(origin)?;
        Fee::<T>::put(fee);
        Ok(())
    }
    // Recall: privileged, but two mutations.
    #[pallet::call_index(2)]
    pub fn reset(origin: OriginFor<T>) -> DispatchResult {
        T::AdminOrigin::ensure_origin(origin)?;
        Fee::<T>::kill();
        Limit::<T>::kill();
        Ok(())
    }
    // Recall: `ensure_none` is not privileged.
    #[pallet::call_index(3)]
    pub fn unsigned_set(origin: OriginFor<T>, fee: u32) -> DispatchResult {
        ensure_none(origin)?;
        Fee::<T>::put(fee);
        Ok(())
    }
    // Recall: a bound origin result is an account, not a privileged check.
    #[pallet::call_index(5)]
    pub fn bound_origin(origin: OriginFor<T>, fee: u32) -> DispatchResult {
        let who = T::ManagerOrigin::ensure_origin(origin)?;
        Fee::<T>::put(fee);
        Ok(())
    }
    // Recall: a root check that falls back to a signed origin.
    #[pallet::call_index(6)]
    pub fn root_or_signed(origin: OriginFor<T>, fee: u32) -> DispatchResult {
        if ensure_root(origin.clone()).is_err() {
            ensure_signed(origin)?;
        }
        Fee::<T>::put(fee);
        Ok(())
    }
    // Recall: `EnsureOriginWithArg` (two arguments).
    #[pallet::call_index(7)]
    pub fn with_arg(origin: OriginFor<T>, asset: u32) -> DispatchResult {
        T::CreateOrigin::ensure_origin(origin, &asset)?;
        Fee::<T>::put(asset);
        Ok(())
    }
    // #15: transient storage, set and killed in one body.
    #[pallet::call_index(4)]
    pub fn with_extra_gas(origin: OriginFor<T>, call: u32) -> DispatchResult {
        ensure_signed(origin)?;
        ExtraGas::<T>::set(Some(1));
        let result = Self::run(call);
        ExtraGas::<T>::kill();
        result
    }
}
"#;
    let diags = check_fixture("pallets/foo/src/lib.rs", code);
    assert_eq!(
        reported_names(&diags, "TST006"),
        vec![
            "bound_origin",
            "reset",
            "root_or_signed",
            "set_own_fee",
            "unsigned_set",
            "with_arg"
        ],
        "{diags:?}"
    );
}

#[test]
fn ben002_counts_checked_results_hook_benchmarks_and_missed_assertions() {
    let code = r#"
#[benchmark]
fn expect_checked() {
    #[block]
    {
        Pallet::<T>::internal(1).expect("succeeds");
    }
}
// #6: hook benchmarks.
#[benchmark]
fn on_initialize_full() {
    #[block]
    {
        Pallet::<T>::on_initialize(1u32.into());
    }
}
// #7: a bound result of a call the matching misses, asserted afterwards.
#[benchmark]
fn bound_result() -> Result<(), BenchmarkError> {
    #[block]
    {
        #[allow(deprecated)]
        let result = unsigned::Pallet::<T>::validate_unsigned(TransactionSource::Local, &call);
        assert_ok!(result);
    }
    Ok(())
}
// #7: assert_matches! after the call.
#[benchmark]
fn matched_event() {
    #[extrinsic_call]
    claim(RawOrigin::Signed(caller));
    assert_matches!(events().last(), Some(Event::Claimed { .. }));
}
// Recall: the asserted value is derived, not the bound result.
#[benchmark]
fn derived_only() {
    #[block]
    {
        let voters = <Pallet<T> as SortedListProvider<T::AccountId>>::iter();
        let len = voters.count();
        assert_eq!(len, 1);
    }
}
// Recall: the unwrap is on a setup read, not on the measured call.
#[benchmark]
fn unwrap_before_call() {
    #[block]
    {
        let x = Foo::<T>::get().unwrap();
        let _ = Pallet::<T>::f(x);
    }
}
// Recall: nothing checked.
#[benchmark]
fn unchecked_call() {
    #[block]
    {
        let _ = Pallet::<T>::internal(1);
    }
}
"#;
    let diags = check_fixture("pallets/foo/src/benchmarking.rs", code);
    assert_eq!(
        reported_names(&diags, "BEN002"),
        vec!["derived_only", "unchecked_call", "unwrap_before_call"],
        "{diags:?}"
    );
}

#[test]
fn ben002_reads_v1_measured_blocks_like_v2() {
    let code = r#"
benchmarks! {
    pause {
        let origin = T::UpdateOrigin::try_successful_origin().unwrap();
    }: {
        assert_ok!(crate::Pallet::<T>::pause(origin));
    }
    revoke {
        let ac = setup::<T>();
    }: { ac.can_revoke().expect("allowed") }
    plain {
        let caller = whitelisted_caller();
    }: { crate::Pallet::<T>::plain(caller) }
}
"#;
    let diags = check_fixture("pallets/foo/src/benchmarking.rs", code);
    assert_eq!(reported_names(&diags, "BEN002"), vec!["plain"], "{diags:?}");
}

#[test]
fn rtc001_skips_parity_test_networks_only() {
    for (package, fires) in [
        ("rococo-runtime", false),
        ("glutton-westend-runtime", false),
        ("westend-runtime", true),
        ("parachain-template-runtime", true),
    ] {
        let krate = rtc_runtime_crate(package, &[("src/lib.rs", RTC_RUNTIME_LIB)]);
        let diags = rtc_scan(krate.path(), polkadot_linter::config::Config::default());
        assert_eq!(
            rtc_findings(&diags) == vec!["RTC001:3".to_string()],
            fires,
            "{package}: {diags:?}"
        );
    }
}

#[test]
fn rtc001_skips_pallets_without_calls() {
    let runtime = r#"
impl pallet_hooks::Config for Runtime {
    type WeightInfo = ();
    type ExtensionsWeightInfo = pallet_hooks::TestWeightInfo;
}
impl pallet_calls::Config for Runtime {
    type WeightInfo = ();
}

construct_runtime!(
    pub enum Runtime {
        Hooks: pallet_hooks = 1,
        Calls: pallet_calls = 2,
    }
);
"#;
    let dir = write_tree(&[
        (
            "runtime/Cargo.toml",
            "[package]\nname = \"chain-runtime\"\nversion = \"0.1.0\"\n\n[dependencies]\npallet-hooks = { path = \"../hooks\" }\npallet-calls = { path = \"../calls\" }\n",
        ),
        ("runtime/src/lib.rs", runtime),
        ("hooks/src/lib.rs", "#[pallet::hooks]\nimpl<T: Config> Hooks for Pallet<T> {}\n"),
        ("calls/src/lib.rs", "#[pallet::call]\nimpl<T: Config> Pallet<T> {}\n"),
    ]);
    let diags = rtc_scan(
        &dir.path().join("runtime"),
        polkadot_linter::config::Config::default(),
    );
    // The no-calls skip covers `()` only, not `TestWeightInfo`.
    let mut findings = rtc_findings(&diags);
    findings.sort();
    assert_eq!(findings, vec!["RTC001:4", "RTC001:7"], "{diags:?}");
}
