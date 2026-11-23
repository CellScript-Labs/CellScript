#[derive(Clone)]
struct Fixture {
    artifact: Vec<u8>,
    metadata: Value,
    record: VerifiedLoweringRecord,
    source_map: SourceArtifactMap,
}

impl Fixture {
    fn new() -> Self {
        let result =
            compile(FIXTURE_SOURCE, CompileOptions { target: Some("riscv64-elf".to_string()), ..CompileOptions::default() }).unwrap();
        Self::from_result(result)
    }

    fn from_result(result: CompileResult) -> Self {
        let fixture = Self {
            artifact: result.artifact_bytes,
            metadata: serde_json::to_value(result.metadata).unwrap(),
            record: result.verified_lowering_record.unwrap(),
            source_map: result.source_artifact_map.unwrap(),
        };
        fixture.check().unwrap();
        fixture
    }

    fn from_source(source: &str) -> Self {
        let result = compile(source, CompileOptions { target: Some("riscv64-elf".to_string()), ..CompileOptions::default() }).unwrap();
        Self::from_result(result)
    }

    fn check(&self) -> Result<(), CheckerRejectionCode> {
        check_bundle_values(&self.artifact, &self.metadata, &self.record, &self.source_map, &CheckerBudgets::default())
            .map(|_| ())
            .map_err(|error| error.code)
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
        self.metadata["verified_artifact"]["lowering_record_hash"] = Value::String(record_hash);
        self.metadata["verified_artifact"]["source_map_hash"] = Value::String(source_map_hash);
        self.metadata["verified_artifact"]["verified_bundle_id"] = Value::String(verified_bundle_id);
    }

    fn rebind_typed_semantics(&mut self) {
        // Preserve deliberately stale or forged inner identities while
        // refreshing their outer record and sidecar hashes.
        self.record.typed_semantics_hash =
            canonical_hash(cellscript_artifact_checker::TYPED_SEMANTICS_SCHEMA, &self.record.typed_semantics).unwrap();
        self.metadata["typed_semantics"] = serde_json::to_value(&self.record.typed_semantics).unwrap();
        self.metadata["typed_semantics_hash"] = Value::String(self.record.typed_semantics_hash.clone());
        self.rebind_sidecars();
    }

    fn rebind_layered_identities(&mut self) {
        // Typed mutations must reach structural and machine checks after
        // all semantic identity layers have been rebound.
        let typed = &mut self.record.typed_semantics;
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
        let roots = foundation
            .provenance
            .nodes
            .iter()
            .filter(|node| !matches!(node.provenance, cellscript_artifact_checker::ValueProvenance::Derived { .. }))
            .map(|node| node.id.as_str())
            .collect::<Vec<_>>();
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
        for (key, value) in [
            ("core_semantic_id", &foundation.identities.core_semantic_id),
            ("entry_contract_id", &foundation.identities.entry_contract_id),
            ("artifact_contract_id", &foundation.identities.artifact_contract_id),
        ] {
            self.metadata["verified_artifact"][key] = Value::String(value.clone());
        }
        self.rebind_typed_semantics();
    }

    fn bind_artifact_identity(&mut self) {
        let artifact_hash = cellscript_artifact_checker::hex_encode(&cellscript_artifact_checker::ckb_blake2b256(&self.artifact));
        self.record.artifact_hash.clone_from(&artifact_hash);
        self.record.artifact_size_bytes = self.artifact.len() as u64;
        self.source_map.artifact_hash.clone_from(&artifact_hash);
        self.metadata["artifact_hash"] = Value::String(artifact_hash);
        self.metadata["artifact_size_bytes"] = Value::from(self.artifact.len() as u64);
        self.metadata["verified_artifact"]["deployable_artifact_id"] = Value::String(self.record.artifact_hash.clone());
        self.rebind_sidecars();
    }

    fn replace_machine_word(&mut self, address: u64, word: u32) {
        self.replace_machine_words(&[(address, word)]);
    }

    fn redirect_conditional_branch(&mut self, address: u64, target: u64) {
        let elf = parse_elf(&self.artifact, CheckerBudgets::default().instructions).unwrap();
        let word = elf.instructions.iter().find(|i| i.address == address).unwrap().word;
        assert_eq!(word & 0x7f, 0x63);
        let origin = self.record.blocks.iter().find(|block| block.range.contains(address)).unwrap().id.clone();
        let destination = self.record.blocks.iter().find(|block| block.range.start == target).unwrap().id.clone();
        self.record.edges.iter_mut().find(|edge| edge.from == origin && edge.kind == EdgeKind::ConditionalTaken).unwrap().to =
            destination;
        let mut reachable = std::collections::BTreeSet::new();
        let mut pending = self.record.entries.iter().map(|entry| entry.entry_block.clone()).collect::<Vec<_>>();
        while let Some(id) = pending.pop() {
            if reachable.insert(id.clone()) {
                pending.extend(self.record.edges.iter().filter(|edge| edge.from == id).map(|edge| edge.to.clone()));
            }
        }
        for block in &mut self.record.blocks {
            block.reachable = reachable.contains(&block.id);
        }
        self.record.canonicalize();
        let displacement = target as i64 - address as i64;
        assert!((-4096..4096).contains(&displacement) && displacement % 2 == 0);
        let immediate = displacement as u32;
        let branch = (word & 0x01fff07f)
            | (((immediate >> 12) & 1) << 31)
            | (((immediate >> 5) & 0x3f) << 25)
            | (((immediate >> 1) & 15) << 8)
            | (((immediate >> 11) & 1) << 7);
        self.replace_machine_word(address, branch);
    }

    fn replace_machine_words(&mut self, replacements: &[(u64, u32)]) {
        let elf = parse_elf(&self.artifact, CheckerBudgets::default().instructions).unwrap();
        for &(address, word) in replacements {
            let offset = (elf.text.offset + address - elf.text.address) as usize;
            self.artifact[offset..offset + 4].copy_from_slice(&word.to_le_bytes());
        }
        for block in &mut self.record.blocks {
            let start = (elf.text.offset + block.range.start - elf.text.address) as usize;
            let end = (elf.text.offset + block.range.end - elf.text.address) as usize;
            block.byte_digest =
                cellscript_artifact_checker::domain_hash_bytes("cellscript-machine-block-v1", &self.artifact[start..end]);
        }
        self.bind_artifact_identity();
    }
}
