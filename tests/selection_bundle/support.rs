//! Shared canonical deployment and transaction fixtures for complete programs.
use super::*;
use ckb_testtool::{
    ckb_types::{bytes::Bytes, core::TransactionBuilder, packed, prelude::*},
    context::Context,
};

pub(super) fn participants(root: &Path, entries: &[(&str, &str, &str)], compiled: &[CompileResult]) -> Vec<Value> {
    assert_eq!(entries.len(), compiled.len());
    let mut artifacts = Vec::new();
    for (index, ((id, kind, entry), result)) in entries.iter().zip(compiled).enumerate() {
        save(root, id, result);
        let sdk = format!("{id}-sdk");
        let generated = Command::new(env!("CARGO_BIN_EXE_cellc"))
            .args(["gen-builder", "--metadata"])
            .arg(root.join(format!("{id}.elf.meta.json")))
            .args(["--target", "typescript", "--output"])
            .arg(root.join(&sdk))
            .output()
            .unwrap();
        assert!(generated.status.success(), "{}", String::from_utf8_lossy(&generated.stderr));
        artifacts.push(json!({
            "id": id, "package_coordinate": format!("example/{id}@1.0.0"), "lock_node_id": format!("{id}-node"),
            "entry": {"kind": kind, "name": entry},
            "script_role": if *kind == "lock" {"lock"} else {"type"},
            "files": {"artifact": format!("{id}.elf"), "metadata": format!("{id}.elf.meta.json"),
                "lowering_record": format!("{id}.elf.lowering.json"), "source_map": format!("{id}.elf.sourcemap.json"),
                "builder_manifest": format!("{sdk}/cellscript-builder-manifest.json")},
            "deployment": {"network": {"chain_id": "ckb-testnet", "genesis_hash": format!("0x{}", "00".repeat(32))},
                "artifact_hash": hex::encode(result.artifact_hash), "script": identity(index as u8 + 1),
                "code_cell_dep": {"out_point": {"tx_hash": format!("0x{}", hex::encode([0xd1 + index as u8; 32])), "index": 0}, "dep_type": "code"}}
        }));
    }
    artifacts
}

pub(super) fn code_identity(tag: u8) -> packed::Script {
    packed::Script::new_builder()
        .code_hash(cellscript::ckb_abi::type_id::CODE_HASH)
        .hash_type(1u8)
        .args(Bytes::from(vec![tag; 32]).pack())
        .build()
}

pub(super) fn identity(tag: u8) -> ProtocolScriptIdentity {
    ProtocolScriptIdentity {
        code_hash: format!("0x{}", hex::encode(code_identity(tag).calc_script_hash().as_slice())),
        hash_type: "type".into(),
        args: "0x00".into(),
    }
}

pub(super) fn packed_script(script: &ProtocolScriptIdentity) -> packed::Script {
    packed::Script::new_builder()
        .code_hash(packed::Byte32::from_slice(&hex::decode(&script.code_hash[2..]).unwrap()).unwrap())
        .hash_type(1u8)
        .args(Bytes::from(hex::decode(&script.args[2..]).unwrap()).pack())
        .build()
}

pub(super) fn compile(source: &str) -> CompileResult {
    compile_at(source, cellscript::CompilerDiagnostics::default())
}

pub(super) fn compile_at(source: &str, diagnostics: cellscript::CompilerDiagnostics) -> CompileResult {
    cellscript::compile_with_executable_surface_policy(
        source,
        CompileOptions {
            edition: NEXT_EDITION,
            diagnostics,
            target: Some("riscv64-elf".into()),
            target_profile: Some("ckb-type-hash".into()),
            ..Default::default()
        },
        cellscript::ExecutableSurfacePolicy::DenyFailClosed,
    )
    .unwrap()
}

/// Exercise the real locked package loader, rather than concatenating a library
/// into the application's source. Public helpers retain their dependency origin.
pub(super) fn compile_package(
    root: &Path,
    application: &str,
    library: &str,
    diagnostics: cellscript::CompilerDiagnostics,
) -> cellscript::error::Result<CompileResult> {
    for (name, source, dependencies) in
        [("rules", library, ""), ("application", application, "\n[dependencies]\nrules = { path = \"../rules\" }\n")]
    {
        let package = root.join(name);
        std::fs::create_dir_all(package.join("src")).unwrap();
        std::fs::write(
            package.join("Cell.toml"),
            format!("[package]\ncellscript_version = \">=0.33.0-alpha.1\"\nname = \"{name}\"\nversion = \"0.1.0\"\nedition = \"2027\"\n{dependencies}"),
        )
        .unwrap();
        std::fs::write(package.join("src/main.cell"), source).unwrap();
    }
    let application = root.join("application");
    let locked = Command::new(env!("CARGO_BIN_EXE_cellc")).current_dir(&application).arg("lock").output().unwrap();
    assert!(locked.status.success(), "{} {}", String::from_utf8_lossy(&locked.stdout), String::from_utf8_lossy(&locked.stderr));
    let lock = std::fs::read(application.join("Cell.lock")).unwrap();
    let compiled = cellscript::compile_path(
        camino::Utf8Path::from_path(&application).unwrap(),
        CompileOptions {
            edition: NEXT_EDITION,
            diagnostics,
            target: Some("riscv64-elf".into()),
            target_profile: Some("ckb-type-hash".into()),
            ..Default::default()
        },
    );
    assert_eq!(std::fs::read(application.join("Cell.lock")).unwrap(), lock);
    let compiled = compiled?;
    assert!(compiled.metadata.source_units.iter().any(|unit| unit.role == "dependency" && unit.path.contains("rules/src/main.cell")));
    Ok(compiled)
}

pub(super) fn type_id(tag: u8) -> ProtocolScriptIdentity {
    ProtocolScriptIdentity {
        code_hash: format!("0x{}", hex::encode(cellscript::ckb_abi::type_id::CODE_HASH)),
        hash_type: "type".into(),
        args: format!("0x{}", hex::encode([tag; 32])),
    }
}

pub(super) fn bind_deployment_lines(input: &mut ProtocolBundleInput, compiled: &[CompileResult], lock_tag: u8) {
    use cellscript::deployment_line_handle::{
        begin_deployment_line, deployment_line_commitment_data, DeploymentLineAdmissionEvidence, DeploymentLineCellEvidence,
        DEPLOYMENT_LINE_ADMISSION_EVIDENCE_SCHEMA,
    };
    use cellscript::protocol_bundle::{ProtocolCellDep, ProtocolDepType, ProtocolOutPoint};
    use cellscript::script_handle::{build_exact_script_handle, compile_metadata_abi_hash, ExactScriptHandleReceiptInput};
    for (index, (artifact, result)) in input.artifacts.iter().zip(compiled).enumerate() {
        let metadata = &result.metadata;
        let (exact, _) = build_exact_script_handle(ExactScriptHandleReceiptInput {
            package_coordinate: &artifact.package_coordinate,
            lock_node_id: &artifact.lock_node_id,
            entry: &artifact.entry,
            script_role: artifact.script_role,
            interface_hash: &metadata.interface_hash,
            typed_semantics_hash: &metadata.typed_semantics_hash,
            artifact_hash: metadata.artifact_hash.as_ref().unwrap(),
            target_profile_hash: &cellscript_artifact_checker::canonical_hash("cellscript-target-profile", &metadata.target_profile)
                .unwrap(),
            runtime_abi_hash: &compile_metadata_abi_hash(metadata).unwrap(),
            verified_bundle_id: metadata.verified_artifact.verified_bundle_id.as_ref().unwrap(),
            deployment: &artifact.deployment,
        })
        .unwrap();
        let admission_type = type_id(0x41 + index as u8);
        let admission_hash = cellscript::script_handle::ckb_script_identity_hash(&admission_type).unwrap();
        let (receipt, handle) = begin_deployment_line(&exact, &metadata.public_interface, &admission_hash).unwrap();
        let data = deployment_line_commitment_data(&handle).unwrap();
        let admission_cell = DeploymentLineCellEvidence {
            out_point: ProtocolOutPoint { tx_hash: format!("0x{}", hex::encode([0xe1 + index as u8; 32])), index: 0 },
            lock: identity(lock_tag),
            type_script: admission_type,
            data_hash: hex::encode(cellscript::ckb_blake2b256(&hex::decode(&data[2..]).unwrap())),
        };
        let code_cell = DeploymentLineCellEvidence {
            out_point: artifact.deployment.code_cell_dep.out_point.clone(),
            lock: identity(lock_tag),
            type_script: type_id(index as u8 + 1),
            data_hash: hex::encode(result.artifact_hash),
        };
        let admission_cell_dep_index = input.transaction.cell_deps.len() as u32;
        input
            .transaction
            .cell_deps
            .push(ProtocolCellDep { out_point: admission_cell.out_point.clone(), dep_type: ProtocolDepType::Code });
        input.deployment_lines.push(DeploymentLineAdmissionEvidence {
            schema: DEPLOYMENT_LINE_ADMISSION_EVIDENCE_SCHEMA.into(),
            version: 1,
            artifact: artifact.id.clone(),
            receipt,
            handle,
            admission_cell,
            admission_cell_data: data,
            admission_cell_dep_index,
            code_cell,
            code_cell_dep_index: index as u32,
        });
    }
}

pub(super) fn point(hash: &str, index: u32) -> packed::OutPoint {
    packed::OutPoint::new_builder()
        .tx_hash(packed::Byte32::from_slice(&hex::decode(&hash[2..]).unwrap()).unwrap())
        .index(index)
        .build()
}

pub(super) fn packed_cell(cell: &cellscript::protocol_bundle::ProtocolCellSlot) -> packed::CellOutput {
    packed::CellOutput::new_builder()
        .capacity(cell.capacity)
        .lock(packed_script(&cell.lock))
        .type_(cell.r#type.as_ref().map(packed_script).pack())
        .build()
}

pub(super) fn context(input: &ProtocolBundleInput, compiled: &[CompileResult], reject: Option<usize>) -> Context {
    let mut context = Context::new_with_deterministic_rng();
    for (index, artifact) in input.artifacts.iter().enumerate() {
        let hostile;
        let result = if reject == Some(index) {
            hostile = compile(if artifact.script_role == cellscript::protocol_bundle::ProtocolScriptRole::Lock {
                "module hostile\nlock authorize() -> bool { return false }"
            } else {
                "module hostile\naction verify() -> u64 { require false return 0 }"
            });
            &hostile
        } else {
            &compiled[index]
        };
        let dep = &artifact.deployment.code_cell_dep.out_point;
        context.create_cell_with_out_point(
            point(&dep.tx_hash, dep.index),
            packed::CellOutput::new_builder()
                .capacity(10_000_000_000_000u64)
                .lock(packed_script(&input.deployment_lines[index].code_cell.lock))
                .type_(Some(code_identity(index as u8 + 1)).pack())
                .build(),
            Bytes::from(result.artifact_bytes.clone()),
        );
    }
    for line in &input.deployment_lines {
        let cell = &line.admission_cell;
        context.create_cell_with_out_point(
            point(&cell.out_point.tx_hash, cell.out_point.index),
            packed::CellOutput::new_builder()
                .capacity(100_000_000_000u64)
                .lock(packed_script(&cell.lock))
                .type_(Some(packed_script(&cell.type_script)).pack())
                .build(),
            Bytes::from(hex::decode(&line.admission_cell_data[2..]).unwrap()),
        );
    }
    for cell in &input.transaction.inputs {
        let out = cell.out_point.as_ref().unwrap();
        context.create_cell_with_out_point(
            point(&out.tx_hash, out.index),
            packed_cell(cell),
            Bytes::from(hex::decode(&cell.data.as_ref().unwrap()[2..]).unwrap()),
        );
    }
    context
}

/// Independent use of CKB's canonical Molecule builders; never copies packed
/// transaction bytes from the adapter under test.
pub(super) fn reference(input: &ProtocolBundleInput) -> Vec<u8> {
    let mut tx = TransactionBuilder::default();
    for cell in &input.transaction.inputs {
        let out = cell.out_point.as_ref().unwrap();
        tx = tx.input(
            packed::CellInput::new_builder().previous_output(point(&out.tx_hash, out.index)).since(cell.since.unwrap()).build(),
        );
    }
    for cell in &input.transaction.outputs {
        tx = tx.output(packed_cell(cell)).output_data(Bytes::from(hex::decode(&cell.data.as_ref().unwrap()[2..]).unwrap()).pack());
    }
    for witness in &input.transaction.witnesses {
        let bytes = witness.input_type_bytes.as_ref().map(|bytes| Bytes::from(hex::decode(&bytes[2..]).unwrap()));
        tx = tx.witness(packed::WitnessArgs::new_builder().input_type(bytes.pack()).build().as_bytes().pack());
    }
    for dep in &input.transaction.cell_deps {
        tx = tx.cell_dep(
            packed::CellDep::new_builder().out_point(point(&dep.out_point.tx_hash, dep.out_point.index)).dep_type(0u8).build(),
        );
    }
    tx.build().data().as_slice().to_vec()
}
