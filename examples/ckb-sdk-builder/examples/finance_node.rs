//! Disposable pinned-node application example. Financial authority remains in
//! ordinary compiled CellScript and upstream secp256k1 Scripts.

#[path = "finance_node/allocation.rs"]
mod allocation;
#[path = "finance_node/backend.rs"]
mod backend;
#[path = "finance_node/bundle.rs"]
mod bundle;
#[path = "finance_node/operations.rs"]
mod operations;
#[path = "finance_node/operations_limits.rs"]
mod operations_limits;
#[path = "finance_node/order.rs"]
mod order;
#[path = "finance_node/protocol.rs"]
mod protocol;
#[path = "../../finance_client/quote.rs"]
#[allow(dead_code)]
mod quote;
#[path = "../../finance_client/recovery.rs"]
#[allow(dead_code)]
mod recovery;
#[path = "finance_node/resource_limits.rs"]
mod resource_limits;
#[path = "finance_node/route.rs"]
mod route;
#[path = "finance_node/settlement.rs"]
mod settlement;

use anyhow::{bail, Context, Result};
use ckb_types::prelude::*;
use serde_json::{json, Value};
use std::{
    fs,
    io::Read,
    path::{Path, PathBuf},
    process::Command,
    time::{Instant, SystemTime, UNIX_EPOCH},
};

struct Options {
    root: PathBuf,
    run: PathBuf,
    ckb_repo: PathBuf,
    ckb_bin: PathBuf,
    cellc: PathBuf,
    cellc_origin: PathBuf,
    cellc_snapshot_hash: String,
    fee: u64,
    nonce: u64,
    a: u128,
    b: u128,
    second: bool,
    include_orders: bool,
    operations: bool,
    c4: bool,
    resource_limits: Option<PathBuf>,
    operations_limits: Option<PathBuf>,
}
impl Options {
    fn parse() -> Result<Self> {
        let root = fs::canonicalize(Path::new(env!("CARGO_MANIFEST_DIR")).join("../.."))?;
        let mut value = Self {
            run: root.join(format!(
                "target/finance-node/{}-{}",
                SystemTime::now().duration_since(UNIX_EPOCH)?.as_millis(),
                std::process::id()
            )),
            ckb_repo: root.join("target/source-contract-acceptance/ckb-pinned"),
            ckb_bin: root.join("target/ckb-cellscript-acceptance/1793922591-production-726194/ckb-runtime/ckb"),
            cellc: root.join("target/debug/cellc"),
            cellc_origin: root.join("target/debug/cellc"),
            cellc_snapshot_hash: String::new(),
            root,
            fee: 30,
            nonce: 1,
            a: 100_000,
            b: 500_000,
            second: false,
            include_orders: true,
            operations: false,
            c4: false,
            resource_limits: None,
            operations_limits: None,
        };
        let mut args = std::env::args().skip(1);
        while let Some(argument) = args.next() {
            let next = args.next().context("arguments require explicit values")?;
            match argument.as_str() {
                "--run-dir" => value.run = next.into(),
                "--ckb-repo" => value.ckb_repo = next.into(),
                "--ckb-bin" => value.ckb_bin = next.into(),
                "--cellc" => value.cellc = next.into(),
                "--fee-bps" => value.fee = next.parse()?,
                "--pool-nonce" => value.nonce = next.parse()?,
                "--reserve-a" => value.a = next.parse()?,
                "--reserve-b" => value.b = next.parse()?,
                "--resource-limits" => value.resource_limits = Some(next.into()),
                "--operations-limits" | "--whole-operation-limits" => value.operations_limits = Some(next.into()),
                "--lane" => {
                    value.operations = next == "operations";
                    value.c4 = next == "complete-c4";
                    value.include_orders = match next.as_str() {
                        "complete" | "complete-c4" | "operations" => true,
                        "pool-route" => false,
                        _ => bail!("lane must be complete, complete-c4, pool-route or operations"),
                    }
                }
                "--profile" => {
                    value.second = match next.as_str() {
                        "reference" => false,
                        "second" => true,
                        _ => bail!("profile must be reference or second"),
                    }
                }
                _ => bail!("unknown option {argument}"),
            }
        }
        if value.operations && (value.second || value.fee != 30 || value.nonce != 1 || value.a != 100_000 || value.b != 500_000) {
            bail!("operations lane uses the fixed reference profile; parameters for its experiments are recorded separately");
        }
        value.ckb_repo = fs::canonicalize(value.ckb_repo)?;
        value.ckb_bin = fs::canonicalize(value.ckb_bin)?;
        value.cellc = fs::canonicalize(value.cellc)?;
        fs::create_dir_all(&value.run)?;
        value.run = fs::canonicalize(value.run)?;
        value.cellc_origin = value.cellc.clone();
        let toolchain = value.run.join("toolchain");
        fs::create_dir_all(&toolchain)?;
        let snapshot = toolchain.join("cellc");
        // Hold the source inode open while copying: another Cargo process may
        // replace the active target path, but cannot change this opened input.
        let mut source = fs::File::open(&value.cellc_origin)?;
        let mut destination = fs::OpenOptions::new().write(true).create_new(true).open(&snapshot)?;
        std::io::copy(&mut source, &mut destination)?;
        destination.sync_all()?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&snapshot, fs::Permissions::from_mode(0o500))?;
        }
        #[cfg(not(unix))]
        fs::set_permissions(&snapshot, source.metadata()?.permissions())?;
        fs::File::open(&toolchain)?.sync_all()?;
        value.cellc = snapshot;
        value.cellc_snapshot_hash = file_hash(&value.cellc)?;
        let pin: Value = serde_json::from_slice(&fs::read(value.root.join("scripts/ckb_acceptance_pin.json"))?)?;
        let head = Command::new("git").arg("-C").arg(&value.ckb_repo).args(["rev-parse", "HEAD"]).output()?;
        if !head.status.success()
            || String::from_utf8(head.stdout)?.trim() != pin["revision"].as_str().context("CKB revision pin missing")?
        {
            bail!("CKB checkout does not match the tracked acceptance pin");
        }
        let binary = Command::new(&value.ckb_bin).arg("--version").output()?;
        let binary_version = String::from_utf8(binary.stdout)?;
        let revision = pin["revision"].as_str().context("CKB revision pin missing")?;
        let expected = format!("ckb {} ({} ", pin["version"].as_str().context("CKB version pin missing")?, &revision[..7]);
        if !binary.status.success() || !binary_version.starts_with(&expected) {
            bail!("CKB binary --version does not match the tracked acceptance pin");
        }
        Ok(value)
    }
}
fn file_hash(path: &Path) -> Result<String> {
    let mut file = fs::File::open(path)?;
    let mut hash = ckb_hash::new_blake2b();
    let mut buffer = [0u8; 65536];
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hash.update(&buffer[..read]);
    }
    let mut digest = [0u8; 32];
    hash.finalize(&mut digest);
    Ok(backend::prefixed(&digest))
}

fn running_application_hash() -> Result<String> {
    // The target/debug path may be replaced by another Cargo build while this
    // process runs. Linux exposes the actual running executable inode here.
    #[cfg(target_os = "linux")]
    {
        file_hash(Path::new("/proc/self/exe"))
    }
    #[cfg(not(target_os = "linux"))]
    {
        file_hash(&std::env::current_exe()?)
    }
}

fn application_source_hashes() -> Value {
    // Compile-time inputs identify the actual application used by this binary,
    // including its reused quote/recovery modules, even in a dirty checkout.
    let sources = [
        ("finance_node.rs", include_str!("finance_node.rs")),
        ("finance_node/backend.rs", include_str!("finance_node/backend.rs")),
        ("finance_node/bundle.rs", include_str!("finance_node/bundle.rs")),
        ("finance_node/protocol.rs", include_str!("finance_node/protocol.rs")),
        ("finance_node/order.rs", include_str!("finance_node/order.rs")),
        ("finance_node/operations.rs", include_str!("finance_node/operations.rs")),
        ("finance_node/operations_limits.rs", include_str!("finance_node/operations_limits.rs")),
        ("finance_node/route.rs", include_str!("finance_node/route.rs")),
        ("finance_node/settlement.rs", include_str!("finance_node/settlement.rs")),
        ("finance_node/allocation.rs", include_str!("finance_node/allocation.rs")),
        ("finance_node/resource_limits.rs", include_str!("finance_node/resource_limits.rs")),
        ("finance_client/quote.rs", include_str!("../../finance_client/quote.rs")),
        ("finance_client/recovery.rs", include_str!("../../finance_client/recovery.rs")),
        ("finance_client/journal.rs", include_str!("../../finance_client/journal.rs")),
    ];
    sources
        .into_iter()
        .map(|(name, source)| (name.to_owned(), json!(backend::prefixed(&ckb_hash::blake2b_256(source.as_bytes())))))
        .collect()
}

fn original_observation(value: &Value) -> Result<cellscript_ckb_adapter::recovery::RecoveryObservation> {
    use cellscript_ckb_adapter::recovery::{RecoveryInclusion, RecoveryObservation, RecoveryRejectionStage};
    let status = &value["tx_status"];
    let hash = |value: &Value| -> Result<ckb_types::H256> {
        Ok(ckb_types::H256::from_slice(&hex::decode(value.as_str().context("missing observation hash")?.trim_start_matches("0x"))?)?)
    };
    let integer = |value: &Value| -> Result<u64> {
        Ok(u64::from_str_radix(value.as_str().context("missing observation quantity")?.trim_start_matches("0x"), 16)?)
    };
    Ok(match status["status"].as_str() {
        Some("pending" | "proposed") => RecoveryObservation::Pending,
        Some("committed") => RecoveryObservation::Included(RecoveryInclusion {
            block_hash: hash(&status["block_hash"])?,
            block_number: integer(&status["block_number"])?,
            transaction_index: u32::try_from(integer(&status["tx_index"])?)?,
        }),
        Some("rejected") => RecoveryObservation::Rejected {
            stage: RecoveryRejectionStage::TransactionStatus,
            reason: status["reason"].as_str().unwrap_or("node rejected without a reason").into(),
        },
        Some("unknown") | None => RecoveryObservation::Unknown,
        Some(other) => bail!("unsupported actual node transaction status {other}"),
    })
}

struct OriginalOutcome {
    points: Vec<ckb_types::packed::OutPoint>,
    hash: ckb_types::H256,
    preflight: Value,
}

fn unknown_original(
    node: &mut backend::Backend,
    programs: &protocol::Programs,
    prepared: &bundle::Prepared,
    pool: &protocol::Pool,
    wallet: &protocol::Wallet,
    minimum_received: u128,
) -> Result<OriginalOutcome> {
    use cellscript_ckb_adapter::recovery::{RecoveryPolicy, RecoveryState, RecoverySubject};
    use recovery::{
        AmountEncoding, AssetAmountBound, FinanceIntent, FinanceIntentParameters, FinanceJournal, FinanceRecovery,
        FinanceRecoverySession,
    };
    let owner = node.owner(wallet.key);
    let intent = FinanceIntent::new(FinanceIntentParameters {
        operation_id: ckb_types::H256::from(ckb_hash::blake2b_256(
            [b"finance-node-original-swap-v1".as_slice(), pool.identity.calc_script_hash().as_slice()].concat(),
        )),
        operation: "swap-a-for-b".into(),
        owner: owner.clone(),
        recipient: owner,
        debits: vec![
            AssetAmountBound { asset: pool.assets[0].clone(), encoding: AmountEncoding::U128Le, amount: 1000 },
            AssetAmountBound { asset: pool.assets[1].clone(), encoding: AmountEncoding::U128Le, amount: 0 },
        ],
        credits: vec![AssetAmountBound { asset: pool.assets[1].clone(), encoding: AmountEncoding::U128Le, amount: minimum_received }],
        subjects: vec![RecoverySubject::Type(pool.identity.clone()), RecoverySubject::Lock(pool.controller.clone())],
        maximum_fee_shannons: backend::FEE,
        maximum_capacity_debit_shannons: backend::FEE,
        maximum_occupied_capacity_shannons: 100 * backend::CELL_CAPACITY,
    })?;
    let genesis = ckb_types::H256::from(node.network_domain);
    let policy = RecoveryPolicy { maximum_rebuilds: 1, maximum_observations: 8, required_confirmations: 2 };
    let journal = FinanceJournal::new(node.run_dir.join("journal"))?;
    let mut client = match FinanceRecovery::start_or_resume(intent.clone(), node.chain.clone(), genesis.clone(), policy, &journal)? {
        FinanceRecoverySession::Fresh(client) => client,
        FinanceRecoverySession::ObservationOnly(_) => bail!("new node exercise unexpectedly reused a recorded operation"),
    };
    client.prepare(prepared.candidate(), &prepared.observed_inputs)?;
    let (signed, signing) = prepared.sign(&programs.all(), node)?;
    client.bind_signed(&signed, &signing)?;
    let acceptance = cellscript_ckb_adapter::CkbSdkAcceptance::new(&node.rpc);
    let dry = acceptance.dry_run_signed_protocol_bundle(&signed, &prepared.materialization, &signing)?;
    let tx_pool = acceptance.test_signed_protocol_bundle(&signed, &prepared.materialization, &signing, &dry)?;
    let preflight = serde_json::to_value(&tx_pool)?;
    let durable = client.begin_submission(&tx_pool, &journal)?;
    let started = Instant::now();
    let actual_ack = acceptance.submit_signed_protocol_bundle(&signed, &signing, &tx_pool)?;
    let send_ms = started.elapsed().as_secs_f64() * 1000.0;
    // Deliberate caller fault injection after a REAL accepted send. This is
    // not presented as an organically observed network RPC timeout.
    client.submission_unknown()?;
    drop(client);
    let mut observer = match FinanceRecovery::start_or_resume(intent, node.chain.clone(), genesis.clone(), policy, &journal)? {
        FinanceRecoverySession::ObservationOnly(observer) => observer,
        FinanceRecoverySession::Fresh(_) => bail!("journal restart regained economic execution rights"),
    };
    let hash = observer.original_transaction_hash().clone();
    let before = node.node.rpc("get_transaction", vec![json!(backend::prefixed(hash.as_bytes()))])?;
    observer.observe(&node.chain, &genesis, &hash, original_observation(&before)?)?;
    let points=node.complete_submitted(&signed,"builder-a-swap-after-discarded-send-ack",json!({"formal_bundle":prepared.report,"signing":signing,"signed_node_execution":dry,"tx_pool":tx_pool,"actual_successful_send_ack":actual_ack,"send_ms":send_ms}))?;
    let after = node.node.rpc("get_transaction", vec![json!(backend::prefixed(hash.as_bytes()))])?;
    let included = original_observation(&after)?;
    observer.observe(&node.chain, &genesis, &hash, included.clone())?;
    node.node.rpc("generate_block", vec![])?;
    let tip = node.node.rpc("get_tip_header", vec![])?;
    let tip_number = u64::from_str_radix(tip["number"].as_str().context("tip number missing")?.trim_start_matches("0x"), 16)?;
    let tip_hash =
        ckb_types::H256::from_slice(&hex::decode(tip["hash"].as_str().context("tip hash missing")?.trim_start_matches("0x"))?)?;
    let cellscript_ckb_adapter::recovery::RecoveryObservation::Included(inclusion) = included else {
        bail!("original transaction is not actually committed");
    };
    let observed = observer.observe_confirmation(&node.chain, &genesis, &hash, inclusion, tip_hash, tip_number)?;
    if observer.state() != RecoveryState::Confirmed {
        bail!("original transaction required depth not observed");
    }
    node.records.push(json!({"case":"durable-unknown-submission-follows-original-actual-node-transaction","status":"original-committed-depth-observed",
        "fault_injection":"discarded-successful-send-acknowledgement; not an observed RPC timeout","journal":durable,"before_mining":before,"after_commit":after,
        "original_transaction_hash":backend::prefixed(hash.as_bytes()),"confirmation_count":observed.confirmation_count,"required_confirmations":observed.required_confirmation_count,
        "observations":observer.observations(),"new_economic_operation_executed":false,"signer_called_after_restart":false,"finality_claim":"bounded-observation-not-absolute-finality"}));
    Ok(OriginalOutcome { points, hash, preflight })
}

fn checked_operation(
    node: &mut backend::Backend,
    programs: &protocol::Programs,
    prepared: &bundle::Prepared,
    intent: recovery::FinanceIntent,
    name: &str,
) -> Result<Vec<ckb_types::packed::OutPoint>> {
    let operation_started = Instant::now();
    let journal = recovery::FinanceJournal::new(node.run_dir.join("journal"))?;
    let policy =
        cellscript_ckb_adapter::recovery::RecoveryPolicy { maximum_rebuilds: 1, maximum_observations: 8, required_confirmations: 1 };
    let mut client = match recovery::FinanceRecovery::start_or_resume(
        intent,
        node.chain.clone(),
        ckb_types::H256::from(node.network_domain),
        policy,
        &journal,
    )? {
        recovery::FinanceRecoverySession::Fresh(client) => client,
        recovery::FinanceRecoverySession::ObservationOnly(_) => bail!("existing operation must be observed rather than re-executed"),
    };
    client.prepare(prepared.candidate(), &prepared.observed_inputs)?;
    let recovery_prepare_ms = operation_started.elapsed().as_secs_f64() * 1000.0;
    let started = Instant::now();
    let (signed, signing) = prepared.sign(&programs.all(), node)?;
    client.bind_signed(&signed, &signing)?;
    let sign_ms = started.elapsed().as_secs_f64() * 1000.0;
    let acceptance = cellscript_ckb_adapter::CkbSdkAcceptance::new(&node.rpc);
    let started = Instant::now();
    let dry = acceptance.dry_run_signed_protocol_bundle(&signed, &prepared.materialization, &signing)?;
    let execution_ms = started.elapsed().as_secs_f64() * 1000.0;
    let started = Instant::now();
    let pool = acceptance.test_signed_protocol_bundle(&signed, &prepared.materialization, &signing, &dry)?;
    let tx_pool_ms = started.elapsed().as_secs_f64() * 1000.0;
    let started = Instant::now();
    let durable = client.begin_submission(&pool, &journal)?;
    let journal_ms = started.elapsed().as_secs_f64() * 1000.0;
    let started = Instant::now();
    let submission = acceptance.submit_signed_protocol_bundle(&signed, &signing, &pool)?;
    client.submitted(&submission)?;
    let send_ms = started.elapsed().as_secs_f64() * 1000.0;
    let points=node.complete_submitted(&signed,name,json!({"formal_bundle":prepared.report,"signing":signing,"signed_node_execution":dry,"tx_pool":pool,"submission":submission,"journal":durable}))?;
    let started = Instant::now();
    let confirmed = cellscript_ckb_adapter::CkbSdkAcceptance::new(&node.rpc).wait_for_protocol_bundle_confirmation(
        &submission,
        &prepared.materialization,
        1,
        8,
        50,
    )?;
    client.confirmed(&confirmed)?;
    node.records.last_mut().context("missing committed case")?["checked_confirmation"] = serde_json::to_value(confirmed)?;
    node.records.last_mut().context("missing committed case")?["client_timings_ms"] = json!({"intent_recovery_prepare":recovery_prepare_ms,
        "sdk_sign_and_receipt":sign_ms,"signed_execution_rpc_and_receipt":execution_ms,"tx_pool_rpc_and_receipt":tx_pool_ms,
        "durable_journal":journal_ms,"submission_rpc_and_receipt":send_ms,"canonical_confirmation_rpc":started.elapsed().as_secs_f64()*1000.0,
        "operation_after_materialization":operation_started.elapsed().as_secs_f64()*1000.0,
        "measurement_scope":"one-real-run-per-case-including-explicit-example-mining; not-a-latency-percentile"});
    Ok(points)
}

fn operation_id(label: &str, pool: &protocol::Pool) -> ckb_types::H256 {
    ckb_types::H256::from(ckb_hash::blake2b_256(
        [label.as_bytes(), pool.identity.calc_script_hash().as_slice(), &pool.revision.to_le_bytes()].concat(),
    ))
}

fn consolidate_wallet(
    programs: &protocol::Programs,
    node: &mut backend::Backend,
    wallet: &mut protocol::Wallet,
    assets: &[ckb_types::packed::Script; 2],
) -> Result<()> {
    use recovery::{AmountEncoding, AssetAmountBound, FinanceIntent, FinanceIntentParameters};
    let owner = node.owner(wallet.key);
    let mut inputs = vec![wallet.funding()?];
    let mut amounts = [0u128; 2];
    let mut witnesses = vec![ckb_types::packed::WitnessArgs::default()];
    for (index, asset) in assets.iter().enumerate() {
        let mut points = node
            .live
            .iter()
            .filter(|(_, (output, _))| output.lock() == owner && output.type_().to_opt().as_ref() == Some(asset))
            .map(|(point, _)| point.clone())
            .collect::<Vec<_>>();
        points.sort_by(|a, b| a.as_slice().cmp(b.as_slice()));
        if points.is_empty() {
            bail!("asset consolidation has no actual owner Cells");
        }
        for (position, point) in points.into_iter().enumerate() {
            let bytes = &node.live[&point].1;
            let amount = u128::from_le_bytes(bytes.as_ref().try_into().context("owner asset is not exact sixteen-byte amount")?);
            amounts[index] = amounts[index].checked_add(amount).context("consolidated asset overflow")?;
            inputs.push(point);
            witnesses.push(if position == 0 { programs.asset.witness(&[])? } else { ckb_types::packed::WitnessArgs::default() });
        }
    }
    let unsigned = node.build(
        &inputs,
        vec![
            protocol::token(programs, node, &assets[0], wallet.key, amounts[0])?,
            protocol::token(programs, node, &assets[1], wallet.key, amounts[1])?,
            backend::cell(owner.clone(), None, vec![]),
        ],
        2,
        witnesses,
    )?;
    let bound =
        |asset: &ckb_types::packed::Script| AssetAmountBound { asset: asset.clone(), encoding: AmountEncoding::U128Le, amount: 0 };
    let intent = FinanceIntent::new(FinanceIntentParameters {
        operation_id: ckb_types::H256::from(ckb_hash::blake2b_256(
            [b"consolidate-actual-owner-assets".as_slice(), unsigned.hash().as_slice()].concat(),
        )),
        operation: "consolidate-actual-owner-assets".into(),
        owner: owner.clone(),
        recipient: owner,
        debits: assets.iter().map(bound).collect(),
        credits: assets.iter().map(bound).collect(),
        subjects: assets.iter().cloned().map(cellscript_ckb_adapter::recovery::RecoverySubject::Type).collect(),
        maximum_fee_shannons: backend::FEE,
        maximum_capacity_debit_shannons: backend::FEE,
        maximum_occupied_capacity_shannons: 100 * backend::CELL_CAPACITY,
    })?;
    let prepared = bundle::prepare(&node.run_dir.join("artifacts"), &programs.all(), node, &unsigned)?;
    let points =
        checked_operation(node, programs, &prepared, intent, "actual-owner-consolidates-all-asset-cells-with-zero-net-spend")?;
    wallet.a = points[0].clone();
    wallet.b = points[1].clone();
    Ok(())
}

fn execute(options: &Options) -> Result<()> {
    let started = Instant::now();
    let profile = resource_limits::Profile {
        name: if options.second { "second" } else { "reference" }.into(),
        lane: if options.operations {
            "operations"
        } else if options.c4 {
            "complete-c4"
        } else if options.include_orders {
            "complete"
        } else {
            "pool-route"
        }
        .into(),
        fee_bps: options.fee,
        pool_nonce: options.nonce,
        reserve_a: options.a.to_string(),
        reserve_b: options.b.to_string(),
    };
    let limits = options.resource_limits.as_ref().map(|path| resource_limits::Limits::load(path, &profile)).transpose()?;
    let operations_limits = options.operations_limits.as_ref().map(|path| operations_limits::Limits::load(path)).transpose()?;
    let mut protocol_elf_hashes = std::collections::BTreeMap::new();
    let mut operational_recovery = None;
    let mut keys = if options.second { vec![0x63, 0x64, 0x66] } else { vec![0x61, 0x62, 0x65] };
    if options.c4 {
        keys.push(if options.second { 0x69 } else { 0x67 });
    }
    eprintln!("[finance-node] starting disposable pinned node");
    let mut node = backend::Backend::start(options.ckb_repo.clone(), options.ckb_bin.clone(), options.run.clone(), keys)?;
    node.c4 = options.c4;
    let result = (|| -> Result<()> {
        eprintln!("[finance-node] compiling public finance packages for genesis {}", node.genesis);
        let programs = protocol::Programs::compile(&options.root, &node.run_dir, &options.cellc, &node, options.include_orders)?;
        for program in programs.all() {
            protocol_elf_hashes.insert(program.name.clone(), backend::prefixed(&ckb_hash::blake2b_256(program.code())));
        }
        eprintln!("[finance-node] deploying real verifiers and bootstrapping signed wallets");
        let (mut wallet, mut secondary) = protocol::bootstrap(&programs, &mut node)?;
        eprintln!("[finance-node] issuing two genuinely enforcing assets");
        let assets = protocol::issue_assets(&programs, &mut node, &mut wallet, &mut secondary)?;
        if options.operations {
            operational_recovery = Some(operations::run(&mut node, &programs, &mut wallet, &mut secondary, assets)?);
            return Ok(());
        }
        eprintln!("[finance-node] creating authentic pool and LP supply");
        let clock = operations::Clock::start(&mut node, &programs, &[wallet.a.clone(), wallet.b.clone()])?;
        let (mut pool, unsigned) =
            protocol::initialise(&programs, &mut node, &mut wallet, assets, options.fee, options.nonce, options.a, options.b)?;
        let prepared = bundle::prepare(&options.run.join("artifacts"), &programs.all(), &node, &unsigned)?;
        use cellscript_ckb_adapter::recovery::RecoverySubject;
        use recovery::{AmountEncoding, AssetAmountBound, FinanceIntent, FinanceIntentParameters};
        let share_profile = AmountEncoding::PoolShares {
            pool: pool.identity.calc_script_hash().unpack(),
            controller_code: pool.controller.code_hash().unpack(),
        };
        let intent = FinanceIntent::new(FinanceIntentParameters {
            operation_id: operation_id("initialise", &pool),
            operation: "initialise-pool".into(),
            owner: node.owner(wallet.key),
            recipient: node.owner(wallet.key),
            debits: vec![
                AssetAmountBound { asset: pool.assets[0].clone(), encoding: AmountEncoding::U128Le, amount: options.a },
                AssetAmountBound { asset: pool.assets[1].clone(), encoding: AmountEncoding::U128Le, amount: options.b },
            ],
            credits: vec![AssetAmountBound {
                asset: pool.lp.clone(),
                encoding: share_profile,
                amount: pool.state.total_lp - quote::LOCKED_MINIMUM,
            }],
            subjects: vec![RecoverySubject::Lock(pool.controller.clone())],
            maximum_fee_shannons: backend::FEE,
            maximum_capacity_debit_shannons: 3 * backend::CELL_CAPACITY + backend::FEE,
            maximum_occupied_capacity_shannons: 100 * backend::CELL_CAPACITY,
        })?;
        let points = checked_operation(&mut node, &programs, &prepared, intent, "authentic-pool-and-lp-initialisation")?;
        pool.points = points[..3].to_vec();
        wallet.a = points[3].clone();
        wallet.b = points[4].clone();
        wallet.lp = Some(points[5].clone());
        clock.record(&mut node, &programs, "pool-initialise")?;
        eprintln!("[finance-node] two independent candidate builders quote the same live predecessor");
        let fund_a = wallet.funding()?;
        let fund_b = wallet.funding()?;
        let (candidate_a, next, received_a) =
            protocol::swap(&programs, &node, &pool, &wallet, fund_a, 1_000, quote::Direction::AForB)?;
        let (candidate_b, _, received_b) = protocol::swap(&programs, &node, &pool, &wallet, fund_b, 2_000, quote::Direction::AForB)?;
        let prepared_a = bundle::prepare(&options.run.join("artifacts"), &programs.all(), &node, &candidate_a)?;
        let candidate_b = node.sign(&candidate_b)?;
        let contention_start = node.elapsed_ms();
        let before_b = node.node.rpc(
            "test_tx_pool_accept",
            vec![serde_json::to_value(cellscript_ckb_adapter::to_rpc_transaction(&candidate_b))?, json!("passthrough")],
        )?;
        let predecessor = pool.points.iter().map(|point| backend::prefixed(point.as_slice())).collect::<Vec<_>>();
        let original = unknown_original(&mut node, &programs, &prepared_a, &pool, &wallet, received_a)?;
        let points = original.points;
        pool.points = points[..3].to_vec();
        pool.state = next;
        pool.revision += 1;
        wallet.a = points[3].clone();
        wallet.b = points[4].clone();
        let stale = node.node.rpc(
            "test_tx_pool_accept",
            vec![serde_json::to_value(cellscript_ckb_adapter::to_rpc_transaction(&candidate_b))?, json!("passthrough")],
        );
        let error = stale.expect_err("the same predecessor cannot be spent twice");
        let stale_rpc_error = backend::Backend::require_stale_input(&error)?;
        let stale_submission = node.submit_stale_original(&candidate_b)?;
        let contention_window = node.contention_window(contention_start, "serialized-candidates-share-pool-predecessor");
        node.records.push(json!({"case":"two-builders-one-authentic-pool-predecessor","status":"conflict-observed",
            "shared_pool_predecessor":predecessor,"candidate_a":backend::prefixed(original.hash.as_bytes()),"candidate_b":backend::prefixed(candidate_b.hash().as_slice()),
            "candidate_a_presend_acceptance":original.preflight,"candidate_b_presend_acceptance":before_b,"candidate_b_after_commit_error":error.to_string(),
            "classification":"stale-input-not-financial-script-rejection","quote_a_received":received_a.to_string(),"quote_b_received":received_b.to_string(),
            "new_economic_retry_executed":false,"stale_rpc_error":stale_rpc_error,"stale_submission":stale_submission,"contention_window":contention_window}));
        eprintln!("[finance-node] reverse-direction swap through the same public adapter");
        let clock = operations::Clock::start(&mut node, &programs, &pool.points)?;
        let funding = wallet.funding()?;
        let (reverse, next, received) = protocol::swap(&programs, &node, &pool, &wallet, funding, 1000, quote::Direction::BForA)?;
        let prepared = bundle::prepare(&options.run.join("artifacts"), &programs.all(), &node, &reverse)?;
        let intent = FinanceIntent::new(FinanceIntentParameters {
            operation_id: operation_id("reverse-swap", &pool),
            operation: "swap-b-for-a".into(),
            owner: node.owner(wallet.key),
            recipient: node.owner(wallet.key),
            debits: vec![
                AssetAmountBound { asset: pool.assets[0].clone(), encoding: AmountEncoding::U128Le, amount: 0 },
                AssetAmountBound { asset: pool.assets[1].clone(), encoding: AmountEncoding::U128Le, amount: 1000 },
            ],
            credits: vec![AssetAmountBound { asset: pool.assets[0].clone(), encoding: AmountEncoding::U128Le, amount: received }],
            subjects: vec![RecoverySubject::Type(pool.identity.clone()), RecoverySubject::Lock(pool.controller.clone())],
            maximum_fee_shannons: backend::FEE,
            maximum_capacity_debit_shannons: backend::FEE,
            maximum_occupied_capacity_shannons: 100 * backend::CELL_CAPACITY,
        })?;
        let points = checked_operation(&mut node, &programs, &prepared, intent, "authentic-reverse-swap")?;
        pool.points = points[..3].to_vec();
        pool.state = next;
        pool.revision += 1;
        wallet.a = points[3].clone();
        wallet.b = points[4].clone();
        clock.record(&mut node, &programs, "pool-reverse-swap")?;
        for stage in 0..2 {
            let clock = operations::Clock::start(&mut node, &programs, &pool.points)?;
            let held = protocol::value(&node, wallet.lp.as_ref().context("missing actual LP")?);
            let burn = if stage == 0 { 10_000.min(held) } else { held };
            eprintln!("[finance-node] redeeming {burn} actual circulating LP (stage {stage})");
            let funding = wallet.funding()?;
            let (unsigned, quoted) = protocol::withdraw(&programs, &node, &pool, &wallet, funding, burn)?;
            let prepared = bundle::prepare(&options.run.join("artifacts"), &programs.all(), &node, &unsigned)?;
            let intent = FinanceIntent::new(FinanceIntentParameters {
                operation_id: operation_id("redeem", &pool),
                operation: "redeem-owned-lp".into(),
                owner: node.owner(wallet.key),
                recipient: node.owner(wallet.key),
                debits: vec![AssetAmountBound { asset: pool.lp.clone(), encoding: share_profile, amount: burn }],
                credits: vec![
                    AssetAmountBound { asset: pool.assets[0].clone(), encoding: AmountEncoding::U128Le, amount: quoted.paid_a },
                    AssetAmountBound { asset: pool.assets[1].clone(), encoding: AmountEncoding::U128Le, amount: quoted.paid_b },
                ],
                subjects: vec![RecoverySubject::Type(pool.identity.clone()), RecoverySubject::Lock(pool.controller.clone())],
                maximum_fee_shannons: backend::FEE,
                maximum_capacity_debit_shannons: backend::FEE,
                maximum_occupied_capacity_shannons: 100 * backend::CELL_CAPACITY,
            })?;
            let points = checked_operation(
                &mut node,
                &programs,
                &prepared,
                intent,
                if stage == 0 { "partial-authentic-lp-redemption" } else { "all-circulating-lp-redeemed-locked-minimum-retained" },
            )?;
            pool.points = points[..3].to_vec();
            pool.state = quoted.next;
            pool.revision += 1;
            wallet.a = points[3].clone();
            wallet.b = points[4].clone();
            wallet.lp = if held > burn { Some(points[5].clone()) } else { None };
            clock.record(
                &mut node,
                &programs,
                if stage == 0 { "pool-partial-redemption" } else { "pool-all-circulating-redemption" },
            )?;
        }
        consolidate_wallet(&programs, &mut node, &mut wallet, &pool.assets)?;
        if options.include_orders {
            order::run(&programs, &mut node, &mut wallet, &mut secondary, pool.assets.clone())?;
        }
        route::run(&programs, &mut node, &mut wallet, pool.assets.clone())?;
        if options.include_orders {
            settlement::run(&programs, &mut node, &mut wallet, &mut secondary, pool.assets.clone())?;
            allocation::run(&programs, &mut node, &mut wallet, &mut secondary, pool.assets.clone())?;
        }
        let mut asset_totals = vec![];
        for asset in &pool.assets {
            let total = node
                .live
                .values()
                .filter(|(output, _)| output.type_().to_opt().as_ref() == Some(asset))
                .map(|(_, data)| u128::from_le_bytes(data[..16].try_into().unwrap()))
                .sum::<u128>();
            if total != 1_000_000 {
                bail!("actual live asset total changed: {total}");
            }
            asset_totals.push(total.to_string());
        }
        let supply = node
            .live
            .values()
            .filter(|(output, _)| output.type_().to_opt().as_ref() == Some(&pool.lp))
            .map(|(_, data)| u128::from_le_bytes(data[..16].try_into().unwrap()))
            .sum::<u128>();
        let circulating = node
            .live
            .values()
            .filter(|(output, _)| output.type_().to_opt().as_ref() == Some(&pool.lp) && output.lock() != pool.minimum)
            .map(|(_, data)| u128::from_le_bytes(data[..16].try_into().unwrap()))
            .sum::<u128>();
        if supply != quote::LOCKED_MINIMUM || pool.state.total_lp != supply || circulating != 0 {
            bail!("final actual LP supply does not equal the locked minimum");
        }
        node.records.push(json!({"case":"continuous-actual-node-asset-and-lp-accounting","status":"matched","asset_totals":asset_totals,"lp_total":supply.to_string(),"pool_supply":pool.state.total_lp.to_string(),"circulating_lp":"0","node_outpoint_lineage":true}));
        Ok(())
    })();
    let result = result.and_then(|()| {
        if file_hash(&options.cellc)? != options.cellc_snapshot_hash {
            bail!("compiler snapshot changed during execution");
        }
        Ok(())
    });
    let mut report = json!({"schema":"cellscript-finance-node-example-v1","status":if result.is_ok(){"passed"}else{"failed"},
        "error":result.as_ref().err().map(|error|format!("{error:#}")),"chain_id":node.chain,"genesis_header_hash":node.genesis,"network_domain":backend::prefixed(&node.network_domain),
        "ckb_repo":options.ckb_repo,"ckb_binary":options.ckb_bin,"ckb_binary_hash":file_hash(&options.ckb_bin)?,
        "compiler_binary":options.cellc,"compiler_binary_hash":file_hash(&options.cellc)?,"compiler_version":cellscript::VERSION,
        "elf_compiler":"cellscript::package::frozen_interface::compile_module-linked-into-application-binary",
        "compiler_cli_roles":["intentional-snapshot-lock-repin","generated-typescript-sdk-manifests"],
        "application_binary_hash":running_application_hash()?,"application_compiled_source_hashes":application_source_hashes(),
        "profile":if options.second{"second"}else{"reference"},"fee_bps":options.fee,"pool_nonce":options.nonce,
        "selected_lane":if options.operations{"operations"}else if options.c4{"complete-c4"}else if options.include_orders{"complete"}else{"pool-route"},
        "profile_parameters":profile,"protocol_elf_hashes":protocol_elf_hashes,"resource_checker_source_hash":resource_limits::checker_source_hash(),
        "quote_events":node.quote_events.borrow().as_slice(),"packing_build_events":node.packing_build_events.borrow().as_slice(),
        "timing_scopes":{"quote":"wall-clock nanoseconds around each pure public quote closure; includes its validation/math; excludes preparing arguments and external data/witness encoding",
            "packing_build":"successful Backend::build calls: capacity balance, ResolvedActionTx assembly and existing packed materializer; excludes caller pre-encoding, post-build mutation and signing",
            "live_dependencies":"measured in aggregate artifact-check/materialization/live-resolution; this static-seeded profile has no dynamic indexer discovery",
            "whole_operation":"named Clock samples begin before known-predecessor live RPCs, quote/funding/argument construction and canonical data/witness encoding, and end after actual inclusion or the intended late rejection; named funded composite samples include their explicit funding/unwrap transactions; nested phase times overlap",
            "canonical_encoding":"sum of actual compiler-owned Program.data and Program.witness calls, including calls before Backend::build; quote/encoding call counts identify legitimately unused phases",
            "case_timing":"one actual run per profile/case occurrence, including explicit example mining; no percentile or load guarantee"},
        "execution":"real-disposable-pinned-node","signing":"upstream-secp256k1-sighash-issuer-and-holder-groups",
        "bootstrap":"only-mined-untyped-funding-and-code-deployment-use-template-always-success; financial-assets-and-custody-use-their-compiled-enforcing-Scripts",
        "cases":node.records,"total_wall_ms":started.elapsed().as_secs_f64()*1000.0,"frozen_financial_budgets":"separate measured namespaces: v2 case stages and whole-operation samples including canonical data/witness encoding; consult each validation result; dynamic indexer not used; node VM stack/memory unavailable",
        "external_developer_reproduction":"not-performed","security_review":"not-claimed","gates_ci":"not-run"});
    report["compiler_binary_origin"] = json!(options.cellc_origin);
    report["compiler_snapshot_initial_hash"] = json!(options.cellc_snapshot_hash);
    report["compiler_snapshot_unchanged"] = json!(file_hash(&options.cellc)? == options.cellc_snapshot_hash);
    report["application_binary_hash_scope"] = json!("running-process-inode-on-Linux");
    report["operational_recovery"] = json!(operational_recovery);
    report["whole_operation_samples"] = json!(node.full_operation_samples);
    report["host_context"] = operations::provenance(&options.root, &node)?;
    let mut budget_passed = if result.is_ok() {
        match resource_limits::observe(&report, profile) {
            Ok(observations) => {
                report["resource_observations"] = serde_json::to_value(&observations)?;
                report["resource_budget_validation"] = limits.as_ref().map_or_else(
                    || json!({"status":"not-requested","automatic_widening":false}),
                    |limits| limits.check(&report, &observations),
                );
            }
            Err(error) => {
                report["resource_budget_validation"] =
                    json!({"status":"failed","error":format!("resource observation failed: {error:#}")});
            }
        }
        report["resource_budget_validation"]["status"] != "failed"
    } else {
        report["resource_budget_validation"] = json!({"status":"not-evaluated-execution-failed"});
        true
    };
    if result.is_ok() && (options.operations || operations_limits.is_some()) {
        let validation = operations_limits
            .as_ref()
            .map_or_else(|| Ok(json!({"status":"not-requested","automatic_widening":false})), |limits| limits.check(&report))?;
        budget_passed &= validation["status"] != "failed";
        report["operations_budget_validation"] = validation;
    }
    if !budget_passed {
        report["execution_status"] = report["status"].clone();
        report["status"] = json!("failed");
        report["error"] = json!("measured resource ceilings exceeded; limits were not changed");
    }
    let report_path = options.run.join("finance-node-report.json");
    fs::write(&report_path, serde_json::to_vec_pretty(&report)?)?;
    eprintln!("[finance-node] report {}", report_path.display());
    result?;
    if !budget_passed {
        bail!("measured resource ceilings exceeded; see preserved report");
    }
    Ok(())
}
fn main() -> Result<()> {
    execute(&Options::parse()?)
}
