impl Fixture {
    fn new(edition: CellScriptEdition) -> Self {
        Self::new_with(edition, cellscript::CompilerDiagnostics::without_optimizations(), declaration())
    }

    fn new_with(edition: CellScriptEdition, diagnostics: cellscript::CompilerDiagnostics, declaration: ArtifactDeclaration) -> Self {
        Self::new_source_with(SOURCE, edition, diagnostics, declaration)
    }

    fn new_source_with(
        source_text: &str,
        edition: CellScriptEdition,
        diagnostics: cellscript::CompilerDiagnostics,
        declaration: ArtifactDeclaration,
    ) -> Self {
        Self::new_source_with_target(source_text, edition, diagnostics, declaration, "ckb")
    }

    fn new_source_with_target(
        source_text: &str,
        edition: CellScriptEdition,
        diagnostics: cellscript::CompilerDiagnostics,
        declaration: ArtifactDeclaration,
        target: &str,
    ) -> Self {
        let directory = tempfile::tempdir().unwrap();
        let source = directory.path().join("main.cell");
        std::fs::write(&source, source_text).unwrap();
        let result = compile_path_with_executable_surface_policy(
            source.to_str().unwrap(),
            CompileOptions {
                source_contracts: true,
                target_profile: Some(target.into()),
                edition,
                diagnostics,
                target: Some("riscv64-elf".to_string()),
                ..CompileOptions::default()
            },
            Some(CompileEntryScope::Artifact(declaration)),
            ExecutableSurfacePolicy::DenyFailClosed,
        )
        .unwrap_or_else(|error| panic!("{error}\n{source_text}"));
        let fixture = Self {
            artifact: result.artifact_bytes,
            metadata: serde_json::to_value(result.metadata).unwrap(),
            record: result.verified_lowering_record.unwrap(),
            source_map: result.source_artifact_map.unwrap(),
        };
        fixture.check().unwrap();
        fixture
    }

    fn check(&self) -> Result<(), CheckerError> {
        check_bundle_values(&self.artifact, &self.metadata, &self.record, &self.source_map, &CheckerBudgets::default()).map(|_| ())
    }

    fn assert_interface_inspection(&self) {
        cellscript_artifact_checker::interface::inspect_bundle(
            &self.artifact,
            &serde_json::to_vec(&self.metadata).unwrap(),
            &serde_json::to_vec(&self.record).unwrap(),
            &serde_json::to_vec(&self.source_map).unwrap(),
            &CheckerBudgets::default(),
        )
        .expect("unmutated compiler-produced interface must pass inspection");
    }

    fn policy_mut(&mut self) -> &mut PolicyWitnessContract {
        let EntryDispatchContract::PolicyWitnessV1(policy) = &mut self.record.typed_semantics.foundation.entry_contract.dispatch
        else {
            panic!("expected policy dispatch");
        };
        policy
    }

    fn param_mut(&mut self, action: &str, index: usize) -> &mut Value {
        &mut self.metadata["actions"].as_array_mut().unwrap().iter_mut().find(|entry| entry["name"] == action).unwrap()["params"]
            [index]
    }

    fn rebind_sidecars(&mut self) {
        let record_hash = canonical_hash(LOWERING_RECORD_SCHEMA, &self.record).unwrap();
        self.source_map.lowering_record_hash = record_hash.clone();
        let source_map_hash = canonical_hash(SOURCE_MAP_SCHEMA, &self.source_map).unwrap();
        let verified_bundle_id = canonical_hash(
            "cellscript-verified-bundle-id-v1",
            &(
                self.record.artifact_hash.as_str(),
                self.record.typed_semantics_hash.as_str(),
                self.record.compatibility_profile_hash.as_str(),
                record_hash.as_str(),
                source_map_hash.as_str(),
                self.source_map.source_digest.as_str(),
            ),
        )
        .unwrap();
        self.metadata["verified_artifact"]["lowering_record_hash"] = record_hash.into();
        self.metadata["verified_artifact"]["source_map_hash"] = source_map_hash.into();
        self.metadata["verified_artifact"]["verified_bundle_id"] = verified_bundle_id.into();
    }

    fn rebind_interface_identity(&mut self) {
        let canonical = cellscript::canonical_json::canonical_json_value(&self.metadata["public_interface"]);
        let hash = cellscript_artifact_checker::hex_encode(&cellscript_artifact_checker::ckb_blake2b256(
            &serde_json::to_vec(&canonical).unwrap(),
        ));
        self.metadata["interface_hash"] = hash.clone().into();
        self.record.typed_semantics.interface_hash = hash;
        self.rebind_policy_identity();
    }

    fn bind_artifact_identity(&mut self) {
        let artifact_hash = cellscript_artifact_checker::hex_encode(&cellscript_artifact_checker::ckb_blake2b256(&self.artifact));
        self.record.artifact_hash.clone_from(&artifact_hash);
        self.record.artifact_size_bytes = self.artifact.len() as u64;
        self.source_map.artifact_hash.clone_from(&artifact_hash);
        self.metadata["artifact_hash"] = artifact_hash.clone().into();
        self.metadata["artifact_size_bytes"] = (self.artifact.len() as u64).into();
        self.metadata["verified_artifact"]["deployable_artifact_id"] = artifact_hash.into();
        self.rebind_sidecars();
    }

    fn replace_machine_word(&mut self, address: u64, word: u32) {
        let elf = parse_elf(&self.artifact, CheckerBudgets::default().instructions).unwrap();
        let offset = (elf.text.offset + address - elf.text.address) as usize;
        self.artifact[offset..offset + 4].copy_from_slice(&word.to_le_bytes());
        for block in &mut self.record.blocks {
            let start = (elf.text.offset + block.range.start - elf.text.address) as usize;
            let end = (elf.text.offset + block.range.end - elf.text.address) as usize;
            block.byte_digest =
                cellscript_artifact_checker::domain_hash_bytes("cellscript-machine-block-v1", &self.artifact[start..end]);
        }
        self.bind_artifact_identity();
    }

    fn rebind_policy_identity(&mut self) {
        let typed = &mut self.record.typed_semantics;
        typed.canonicalize();
        let foundation = &mut typed.foundation;
        let contract = &mut foundation.entry_contract;
        let previous_node = contract.semantic_node_id.clone();
        contract.semantic_node_id = canonical_hash(
            "cellscript-semantic-node-entry-contract-v2",
            &(
                contract.script_role.as_str(),
                contract.trigger.as_str(),
                contract.exact_entry.as_str(),
                &contract.dispatch,
                contract.entry_payload_abi.as_str(),
                contract.witness_placement_abi.as_str(),
                contract.witness_placement_field.as_str(),
                contract.witness_placement_source.as_str(),
            ),
        )
        .unwrap();
        for mapping in &mut self.source_map.semantic_mappings {
            if mapping.semantic_node_id == previous_node {
                mapping.semantic_node_id = contract.semantic_node_id.clone();
            }
        }
        self.rebind_layered_identities();
    }

    fn rebind_layered_identities(&mut self) {
        let typed = &mut self.record.typed_semantics;
        typed.canonicalize();
        let entry_programs = typed
            .entries
            .iter()
            .map(|entry| {
                (
                    entry.id.as_str(),
                    entry.kind.as_str(),
                    entry.name.as_str(),
                    &entry.params,
                    &entry.cell_bindings,
                    entry.return_type.as_str(),
                    &entry.effects,
                    entry.entry_block,
                    &entry.locals,
                    &entry.blocks,
                    &entry.borrows,
                    &entry.ownership,
                    &entry.obligations,
                    entry.zk_origins.iter().map(|origin| (origin.policy.as_str(), &origin.fields)).collect::<Vec<_>>(),
                )
            })
            .collect::<Vec<_>>();
        let foundation = &mut typed.foundation;
        let roots = foundation
            .provenance
            .nodes
            .iter()
            .filter(|node| !matches!(node.provenance, ValueProvenance::Derived { .. }))
            .map(|node| node.id.as_str())
            .collect::<Vec<_>>();
        foundation.identities.core_semantic_id = canonical_hash(
            "cellscript-core-semantic-id-v3",
            &(
                typed.failure_semantics,
                &typed.types,
                &entry_programs,
                &foundation.roles,
                &foundation.dispositions,
                &foundation.claims,
                &foundation.operation_nodes,
            ),
        )
        .unwrap();
        foundation.identities.entry_contract_id = canonical_hash(
            "cellscript-entry-contract-id-v1",
            &(
                foundation.identities.core_semantic_id.as_str(),
                &foundation.entry_contract,
                roots,
                foundation.entry_contract.entry_payload_abi.as_str(),
                foundation.entry_contract.witness_placement_abi.as_str(),
            ),
        )
        .unwrap();
        foundation.identities.artifact_contract_id = canonical_hash(
            "cellscript-artifact-contract-id-v1",
            &(foundation.identities.entry_contract_id.as_str(), &foundation.artifact_contract),
        )
        .unwrap();
        self.source_map.canonicalize();
        self.metadata["verified_artifact"]["core_semantic_id"] = foundation.identities.core_semantic_id.clone().into();
        self.metadata["verified_artifact"]["entry_contract_id"] = foundation.identities.entry_contract_id.clone().into();
        self.metadata["verified_artifact"]["artifact_contract_id"] = foundation.identities.artifact_contract_id.clone().into();
        self.record.typed_semantics_hash = canonical_hash(TYPED_SEMANTICS_SCHEMA, typed).unwrap();
        self.metadata["typed_semantics"] = serde_json::to_value(typed).unwrap();
        self.metadata["typed_semantics_hash"] = self.record.typed_semantics_hash.clone().into();
        self.rebind_sidecars();
    }
}

fn policy_block<'a>(fixture: &'a Fixture, prefix: &str) -> &'a cellscript_artifact_checker::LoweringBlock {
    fixture
        .record
        .blocks
        .iter()
        .find(|block| {
            block.owner_entry == "wrapper:_cellscript_entry"
                && block.machine_label.as_deref().is_some_and(|label| {
                    label
                        .strip_prefix(prefix)
                        .is_some_and(|suffix| !suffix.is_empty() && suffix.bytes().all(|byte| byte.is_ascii_digit()))
                })
        })
        .unwrap()
}
