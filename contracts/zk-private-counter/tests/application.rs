use ark_std::rand::{rngs::StdRng, SeedableRng};
use cellscript_zk_private_counter::{self as counter, package, wire, CounterCircuit, Witness};
use ckb_testtool::{
    builtin::ALWAYS_SUCCESS,
    ckb_types::{
        bytes::Bytes,
        core::{TransactionBuilder, TransactionView},
        packed,
        prelude::*,
    },
    context::Context,
};
use serde_json::json;
mod support;
use support::*;

#[test]
fn private_authorization_creation_and_successors() {
    let mut rng = StdRng::seed_from_u64(0x43534b43544e); // Public test parameters only.
    let package_dir = std::env::var_os("CELLSCRIPT_COUNTER_PACKAGE");
    let (pk, manifest) = if let Some(path) = &package_dir {
        let (manifest, pk) = package::load_package(std::path::Path::new(path)).unwrap();
        (pk, manifest)
    } else {
        let (pk, _) = counter::setup(&mut rng).unwrap();
        let temp = tempfile::tempdir().unwrap();
        let dir = temp.path().join("setup");
        let manifest = package::write_package(&dir, &pk, package::SetupKind::PublicTestSeed).unwrap();
        let (_, loaded) = package::load_package(&dir).unwrap();
        assert_eq!(counter::serialize(&loaded).unwrap(), counter::serialize(&pk).unwrap());
        let mut vk = std::fs::read(dir.join("verification-key.bin")).unwrap();
        vk[0] ^= 1;
        std::fs::write(dir.join("verification-key.bin"), vk).unwrap();
        assert!(package::load_package(&dir).is_err());
        (pk, manifest)
    };
    let key = counter::serialize(&pk.vk).unwrap();
    let mut context = Context::new_with_deterministic_rng();
    let child = child_bytes();
    let lifecycle = lifecycle_bytes();
    let child_out = context.deploy_cell(Bytes::from(child.clone()));
    let key_out = context.deploy_cell(Bytes::from(key.clone()));
    let (compiled, handle) = parent(&child, &key, &child_out, [0x11; 32]);
    let parent_bytes = cellscript::strip_vm_abi_trailer(&compiled.artifact_bytes);
    let parent_out = context.deploy_cell(Bytes::copy_from_slice(parent_bytes));
    let lifecycle_out = context.deploy_cell(Bytes::from(lifecycle.clone()));
    let lock_out = context.deploy_cell(ALWAYS_SUCCESS.clone());
    let lock = context.build_script(&lock_out, Bytes::new()).unwrap();
    let funding = packed::CellOutput::new_builder().capacity(100_000_000_000u64).lock(lock.clone()).build();
    let funding_out = context.create_cell(funding.clone(), Bytes::new());
    let input = packed::CellInput::new_builder().previous_output(funding_out).build();
    let args = type_args(&input, 0, parent_bytes);
    let script = context.build_script(&lifecycle_out, Bytes::from(args)).unwrap();
    let cell = funding.as_builder().type_(Some(script.clone()).pack()).build();
    let secret = [0x23; 32];
    let owner = counter::owner(&secret);
    let data = |n| Bytes::copy_from_slice(&counter::state(&owner, n));
    let creation =
        context.complete_tx(TransactionBuilder::default().input(input).output(cell.clone()).output_data(data(0).pack()).build());
    let creation_cycles = context.verify_tx(&creation, 250_000_000).unwrap();
    let mut rows = vec![json!({"case":"create","cycles":creation_cycles})];
    reject(&context, &creation.as_advanced_builder().set_outputs_data(vec![data(1).pack()]).build(), 93, "create-nonzero", &mut rows);
    let wrong_script = script.clone().as_builder().args(Bytes::from(vec![0; 64]).pack()).build();
    reject(
        &context,
        &creation.as_advanced_builder().set_outputs(vec![cell.clone().as_builder().type_(Some(wrong_script).pack()).build()]).build(),
        95,
        "create-wrong-type-id",
        &mut rows,
    );
    let mut previous = packed::OutPoint::new_builder().tx_hash(creation.hash()).index(0u32).build();
    context.create_cell_with_out_point(previous.clone(), cell.clone(), data(0));
    let deps = vec![dep(child_out), dep(key_out), dep(parent_out)];
    let mut old_proof: Option<Vec<u8>> = None;
    for n in 0..2 {
        let tx = context.complete_tx(
            TransactionBuilder::default()
                .input(packed::CellInput::new_builder().previous_output(previous.clone()).build())
                .output(cell.clone())
                .output_data(data(n + 1).pack())
                .set_cell_deps(deps.clone())
                .build(),
        );
        let statement = statement(&script, &previous, &data(n), &data(n + 1), &tx);
        let circuit = CounterCircuit { statement: statement.clone(), witness: Witness { secret, old_counter: n, new_counter: n + 1 } };
        let proof = counter::prove(&pk, circuit, &mut rng).unwrap();
        counter::verify(&key, &statement, &proof).unwrap();
        if n == 0 {
            // All public fields must be bound, including otherwise uninterpreted context limbs.
            for i in 0..wire::STATEMENT_BYTES {
                let mut bytes = statement.encode();
                bytes[i] ^= 1;
                let changed = wire::Statement::decode(&bytes).unwrap();
                assert!(counter::verify(&key, &changed, &proof).is_err(), "unbound statement byte {i}");
            }
            let mut malformed = key.clone();
            malformed[224..232].copy_from_slice(&u64::MAX.to_le_bytes());
            assert!(counter::verify(&malformed, &statement, &proof).is_err());
        }
        let with_proof = |p: &[u8]| tx.as_advanced_builder().set_witnesses(vec![witness(&compiled, p, &handle).pack()]).build();
        if let Some(old) = &old_proof {
            reject(&context, &with_proof(old), 79, "successor-replay", &mut rows);
        }
        let tx = with_proof(&proof);
        let cycles = context.verify_tx(&tx, 250_000_000).unwrap();
        rows.push(json!({"case":format!("update-{n}-{}",n+1),"cycles":cycles,"transaction_hash":hex::encode(tx.hash().as_slice()),"bytes":tx.data().as_slice().len()}));
        if n == 0 {
            reject(&context, &tx.as_advanced_builder().set_outputs_data(vec![data(2).pack()]).build(), 79, "skip-counter", &mut rows);
            reject(
                &context,
                &tx.as_advanced_builder().set_outputs_data(vec![Bytes::copy_from_slice(&counter::state(&[7; 32], 1)).pack()]).build(),
                92,
                "change-owner",
                &mut rows,
            );
            reject(
                &context,
                &tx.as_advanced_builder().set_outputs(vec![cell.clone().as_builder().capacity(99_000_000_000u64).build()]).build(),
                92,
                "change-capacity",
                &mut rows,
            );
            reject(&context, &tx.as_advanced_builder().set_outputs(vec![]).set_outputs_data(vec![]).build(), 94, "burn", &mut rows);
            reject(
                &context,
                &tx.as_advanced_builder().output(cell.clone()).output_data(data(1).pack()).build(),
                95,
                "duplicate-state",
                &mut rows,
            );
            let changed_lock = cell.lock().as_builder().args(Bytes::from_static(b"different-recipient").pack()).build();
            reject(
                &context,
                &tx.as_advanced_builder().set_outputs(vec![cell.clone().as_builder().lock(changed_lock).build()]).build(),
                92,
                "change-lock",
                &mut rows,
            );
            reject(
                &context,
                &tx.as_advanced_builder().set_outputs_data(vec![Bytes::from_static(b"CSZKCNT1").pack()]).build(),
                92,
                "truncated-state",
                &mut rows,
            );
            let mut absent = tx.cell_deps().into_iter().collect::<Vec<_>>();
            absent.remove(2);
            reject(&context, &tx.as_advanced_builder().set_cell_deps(absent).build(), 96, "missing-parent", &mut rows);
            let mut corrupt = proof;
            corrupt[0] ^= 1;
            reject(&context, &with_proof(&corrupt), 79, "corrupt-proof", &mut rows);
        }
        previous = packed::OutPoint::new_builder().tx_hash(tx.hash()).index(0u32).build();
        context.create_cell_with_out_point(previous.clone(), cell.clone(), data(n + 1));
        old_proof = Some(proof.to_vec());
    }
    let report = json!({"schema":"cellscript-counter-application-evidence-v1","status":"passed","circuit_sha256":manifest.circuit.r1cs_sha256,
        "circuit":manifest.circuit,"verification_key_data_hash":hex::encode(counter::hash(&key)),"setup_kind":manifest.setup_kind,
        "child_data_hash":hex::encode(counter::hash(&child)),"parent_data_hash":hex::encode(counter::hash(parent_bytes)),
        "lifecycle_data_hash":hex::encode(counter::hash(&lifecycle)),"rows":rows,"production_admitted":false,
        "scope":"CKB testtool scheduler; real private authorization circuit; public test witness; not a chain deployment"});
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("target");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("application-evidence.json"), serde_json::to_vec_pretty(&report).unwrap()).unwrap();
}
fn reject(context: &Context, tx: &TransactionView, code: i8, label: &str, rows: &mut Vec<serde_json::Value>) {
    let error = context.verify_tx(tx, 250_000_000).expect_err(label).to_string();
    assert!(error.contains(&format!("error code {code}")), "{label}: {error}");
    rows.push(json!({"case":label,"error_code":code,"error":error}));
}
