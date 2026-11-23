fn unit_resource_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/resources")
}

fn constraint_object(expected: &[(&str, &str)]) -> Value {
    Value::Object(expected.iter().map(|(key, value)| ((*key).to_string(), Value::String((*value).to_string()))).collect())
}

fn test_hex32(byte: u8) -> String {
    format!("0x{}", format!("{byte:02x}").repeat(32))
}

struct TestBtcProfileMaterial {
    tx_hex: String,
    txid: String,
    wtxid: String,
    binding: Value,
    expected_binding: Value,
}

fn push_test_compact_size(target: &mut Vec<u8>, value: u64) {
    match value {
        0x00..=0xfc => target.push(value as u8),
        0xfd..=0xffff => {
            target.push(0xfd);
            target.extend_from_slice(&(value as u16).to_le_bytes());
        }
        0x1_0000..=0xffff_ffff => {
            target.push(0xfe);
            target.extend_from_slice(&(value as u32).to_le_bytes());
        }
        _ => {
            target.push(0xff);
            target.extend_from_slice(&value.to_le_bytes());
        }
    }
}

fn test_bitcoin_tx_hex(prevouts: &[(String, u32)], outputs: &[(u64, Vec<u8>)]) -> String {
    let mut bytes = Vec::new();
    bytes.extend_from_slice(&2u32.to_le_bytes());
    push_test_compact_size(&mut bytes, prevouts.len() as u64);
    for (prev_txid, prev_vout) in prevouts {
        bytes.extend_from_slice(&bitcoin_internal_hash_from_display(prev_txid).unwrap());
        bytes.extend_from_slice(&prev_vout.to_le_bytes());
        push_test_compact_size(&mut bytes, 0);
        bytes.extend_from_slice(&0xffff_ffffu32.to_le_bytes());
    }
    push_test_compact_size(&mut bytes, outputs.len() as u64);
    for (amount_sats, script_pubkey) in outputs {
        bytes.extend_from_slice(&amount_sats.to_le_bytes());
        push_test_compact_size(&mut bytes, script_pubkey.len() as u64);
        bytes.extend_from_slice(script_pubkey);
    }
    bytes.extend_from_slice(&0u32.to_le_bytes());
    format!("0x{}", hex::encode(bytes))
}

fn test_bitcoin_tx_bytes(prevouts: &[(String, u32)], outputs: &[(u64, Vec<u8>)]) -> Vec<u8> {
    let hex = test_bitcoin_tx_hex(prevouts, outputs);
    hex_bytes(&hex).expect("test transaction hex should decode")
}

fn test_btc_script(seed: u8) -> Vec<u8> {
    vec![0x51, seed, seed.wrapping_add(1)]
}

fn test_anchor_source(profile: &str) -> &'static str {
    match profile {
        EXPECTED_BTC_TX_COMMITMENT_PROFILE => "external_public_btc_transaction",
        EXPECTED_BTC_UTXO_SEAL_PROFILE | EXPECTED_DUAL_SEAL_PROFILE => "external_public_btc_spend",
        _ => "external_public_btc_transaction",
    }
}

fn test_btc_profile_material(profile: &str, seed: u8) -> TestBtcProfileMaterial {
    match profile {
        EXPECTED_BTC_UTXO_SEAL_PROFILE => {
            let sealed_script = test_btc_script(seed);
            let sealed_amount = 75_000 + u64::from(seed);
            let script_pubkey_hash = format!("0x{}", hex::encode(crate::ckb_blake2b256(&sealed_script)));
            let sealed_tx_hex = test_bitcoin_tx_hex(
                &[(test_hex32(seed.wrapping_add(0x30)), 0)],
                &[(10_000, vec![0x51]), (sealed_amount, sealed_script)],
            );
            let sealed_tx = parse_bitcoin_tx_hex(&sealed_tx_hex).unwrap();
            let tx_hex = test_bitcoin_tx_hex(&[(sealed_tx.txid.clone(), 1)], &[(20_000 + u64::from(seed), vec![0x51, seed])]);
            let tx = parse_bitcoin_tx_hex(&tx_hex).unwrap();
            let sealed_utxo_commitment_hash =
                btc_sealed_utxo_commitment_hash(&sealed_tx.txid, 1, sealed_amount, &script_pubkey_hash).unwrap();
            let binding = json!({
                "kind": "btc_utxo_spend",
                "spend_input_index": 0,
                "sealed_btc_txid": sealed_tx.txid,
                "sealed_btc_vout_index": 1,
                "sealed_btc_amount_sats": sealed_amount,
                "script_pubkey_hash": script_pubkey_hash,
                "sealed_btc_tx_hex": sealed_tx_hex,
                "sealed_utxo_commitment_hash": sealed_utxo_commitment_hash,
            });
            let expected_binding = json!({
                "anchor_source": test_anchor_source(profile),
                "btc_txid": tx.txid,
                "btc_wtxid": tx.wtxid,
                "spend_input_index": 0,
                "sealed_btc_txid": binding["sealed_btc_txid"].clone(),
                "sealed_btc_vout_index": 1,
                "sealed_btc_amount_sats": sealed_amount,
                "script_pubkey_hash": binding["script_pubkey_hash"].clone(),
                "sealed_utxo_commitment_hash": binding["sealed_utxo_commitment_hash"].clone(),
            });
            TestBtcProfileMaterial {
                tx_hex,
                txid: json_pointer_str(&expected_binding, "/btc_txid").unwrap().to_string(),
                wtxid: json_pointer_str(&expected_binding, "/btc_wtxid").unwrap().to_string(),
                binding,
                expected_binding,
            }
        }
        EXPECTED_DUAL_SEAL_PROFILE => {
            let sealed_script = test_btc_script(seed.wrapping_add(0x10));
            let sealed_amount = 85_000 + u64::from(seed);
            let script_pubkey_hash = format!("0x{}", hex::encode(crate::ckb_blake2b256(&sealed_script)));
            let sealed_tx_hex = test_bitcoin_tx_hex(
                &[(test_hex32(seed.wrapping_add(0x40)), 0)],
                &[(10_000, vec![0x51]), (sealed_amount, sealed_script)],
            );
            let sealed_tx = parse_bitcoin_tx_hex(&sealed_tx_hex).unwrap();
            let tx_hex = test_bitcoin_tx_hex(&[(sealed_tx.txid.clone(), 1)], &[(30_000 + u64::from(seed), vec![0x51, seed])]);
            let tx = parse_bitcoin_tx_hex(&tx_hex).unwrap();
            let sealed_utxo_commitment_hash =
                btc_sealed_utxo_commitment_hash(&sealed_tx.txid, 1, sealed_amount, &script_pubkey_hash).unwrap();
            let binding = json!({
                "kind": "dual_seal_btc_closure",
                "spend_input_index": 0,
                "sealed_btc_txid": sealed_tx.txid,
                "sealed_btc_vout_index": 1,
                "sealed_btc_amount_sats": sealed_amount,
                "script_pubkey_hash": script_pubkey_hash,
                "sealed_btc_tx_hex": sealed_tx_hex,
                "sealed_utxo_commitment_hash": sealed_utxo_commitment_hash,
            });
            let expected_binding = json!({
                "anchor_source": test_anchor_source(profile),
                "btc_txid": tx.txid,
                "btc_wtxid": tx.wtxid,
                "spend_input_index": 0,
                "sealed_btc_txid": binding["sealed_btc_txid"].clone(),
                "sealed_btc_vout_index": 1,
                "sealed_btc_amount_sats": sealed_amount,
                "script_pubkey_hash": binding["script_pubkey_hash"].clone(),
                "sealed_utxo_commitment_hash": binding["sealed_utxo_commitment_hash"].clone(),
            });
            TestBtcProfileMaterial {
                tx_hex,
                txid: json_pointer_str(&expected_binding, "/btc_txid").unwrap().to_string(),
                wtxid: json_pointer_str(&expected_binding, "/btc_wtxid").unwrap().to_string(),
                binding,
                expected_binding,
            }
        }
        _ => {
            let amount = 50_000 + u64::from(seed);
            let tx_hex = test_bitcoin_tx_hex(
                &[(test_hex32(seed.wrapping_add(0x20)), 0)],
                &[(10_000, vec![0x51]), (20_000, vec![0x51, seed]), (amount, test_btc_script(seed))],
            );
            let tx = parse_bitcoin_tx_hex(&tx_hex).unwrap();
            let binding = json!({
                "kind": "btc_transaction_output",
                "btc_output_index": 2,
                "btc_amount_sats": amount,
            });
            let expected_binding = json!({
                "anchor_source": test_anchor_source(profile),
                "btc_txid": tx.txid,
                "btc_wtxid": tx.wtxid,
                "btc_output_index": 2,
                "btc_amount_sats": amount,
            });
            TestBtcProfileMaterial {
                tx_hex,
                txid: json_pointer_str(&expected_binding, "/btc_txid").unwrap().to_string(),
                wtxid: json_pointer_str(&expected_binding, "/btc_wtxid").unwrap().to_string(),
                binding,
                expected_binding,
            }
        }
    }
}

fn merge_expected_btc_binding_fields(target: &mut Value, material: &TestBtcProfileMaterial) {
    for (key, value) in material.expected_binding.as_object().unwrap() {
        target.as_object_mut().unwrap().insert(key.clone(), value.clone());
    }
}

fn add_expected_btc_request_fields(request: &mut Value, material: &TestBtcProfileMaterial) {
    for (binding_field, request_field) in [
        ("anchor_source", "local_anchor_source"),
        ("anchor_source", "expected_anchor_source"),
        ("btc_txid", "expected_btc_txid"),
        ("btc_wtxid", "expected_btc_wtxid"),
        ("btc_output_index", "expected_btc_output_index"),
        ("btc_amount_sats", "expected_btc_amount_sats"),
        ("spend_input_index", "expected_spend_input_index"),
        ("sealed_btc_txid", "expected_sealed_btc_txid"),
        ("sealed_btc_vout_index", "expected_sealed_btc_vout_index"),
        ("sealed_btc_amount_sats", "expected_sealed_btc_amount_sats"),
        ("script_pubkey_hash", "expected_script_pubkey_hash"),
        ("sealed_utxo_commitment_hash", "expected_sealed_utxo_commitment_hash"),
    ] {
        if let Some(value) = material.expected_binding.get(binding_field) {
            request.as_object_mut().unwrap().insert(request_field.to_string(), value.clone());
        }
    }
}

fn test_btc_spv_material(seed: u8, confirmations: i64, btc: &TestBtcProfileMaterial) -> Value {
    let txid = btc.txid.clone();
    let sibling = test_hex32(seed.wrapping_add(1));
    let merkle_root = bitcoin_merkle_root_display(&txid, std::slice::from_ref(&sibling), 0).unwrap();
    let mut header = vec![0u8; 80];
    header[0..4].copy_from_slice(&2u32.to_le_bytes());
    let merkle_root_internal = bitcoin_internal_hash_from_display(&merkle_root).unwrap();
    header[36..68].copy_from_slice(&merkle_root_internal);
    header[68..72].copy_from_slice(&1_800_000_000u32.to_le_bytes());
    header[72..76].copy_from_slice(&0x1d00ffffu32.to_le_bytes());
    header[76..80].copy_from_slice(&u32::from(seed).to_le_bytes());
    let btc_block_header = format!("0x{}", hex::encode(&header));
    let btc_block_hash = bitcoin_display_hash(&header);
    let block_height = 900_000i64 + i64::from(seed);
    let observed_tip_height = block_height + confirmations - 1;
    let mut material = json!({
        "btc_txid": txid,
        "btc_wtxid": btc.wtxid.clone(),
        "btc_tx_hex": btc.tx_hex.clone(),
        "btc_transaction_binding": btc.binding.clone(),
        "btc_block_hash": btc_block_hash,
        "btc_block_header": btc_block_header,
        "btc_merkle_proof": {
            "tx_index": 0,
            "merkle_branch": [sibling],
            "merkle_root": merkle_root,
            "block_height": block_height,
            "observed_tip_height": observed_tip_height,
        },
    });
    let proof_hash = btc_spv_proof_material_hash(&material).unwrap();
    material["spv_proof_hash"] = json!(proof_hash);
    material
}

fn test_btc_single_tx_block_spv_material(seed: u8, confirmations: i64, btc: &TestBtcProfileMaterial) -> Value {
    let txid = btc.txid.clone();
    let mut header = vec![0u8; 80];
    header[0..4].copy_from_slice(&2u32.to_le_bytes());
    let merkle_root_internal = bitcoin_internal_hash_from_display(&txid).unwrap();
    header[36..68].copy_from_slice(&merkle_root_internal);
    header[68..72].copy_from_slice(&1_800_000_000u32.to_le_bytes());
    header[72..76].copy_from_slice(&0x1d00ffffu32.to_le_bytes());
    header[76..80].copy_from_slice(&u32::from(seed).to_le_bytes());
    let btc_block_header = format!("0x{}", hex::encode(&header));
    let btc_block_hash = bitcoin_display_hash(&header);
    let block_height = 910_000i64 + i64::from(seed);
    let observed_tip_height = block_height + confirmations - 1;
    let mut material = json!({
        "btc_txid": txid,
        "btc_wtxid": btc.wtxid.clone(),
        "btc_tx_hex": btc.tx_hex.clone(),
        "btc_transaction_binding": btc.binding.clone(),
        "btc_block_hash": btc_block_hash,
        "btc_block_header": btc_block_header,
        "btc_merkle_proof": {
            "tx_index": 0,
            "merkle_branch": [],
            "merkle_root": txid,
            "block_height": block_height,
            "observed_tip_height": observed_tip_height,
        },
    });
    let proof_hash = btc_spv_proof_material_hash(&material).unwrap();
    material["spv_proof_hash"] = json!(proof_hash);
    material
}

fn set_json_pointer_string(value: &mut Value, pointer: &str, content: String) {
    let mut current = value;
    let mut parts = pointer.trim_start_matches('/').split('/').peekable();
    while let Some(part) = parts.next() {
        if parts.peek().is_none() {
            current.as_object_mut().unwrap().insert(part.to_string(), Value::String(content));
            return;
        }
        current = current.as_object_mut().unwrap().entry(part.to_string()).or_insert_with(|| json!({}));
    }
}

fn set_json_pointer_value(value: &mut Value, pointer: &str, content: Value) {
    let mut current = value;
    let mut parts = pointer.trim_start_matches('/').split('/').peekable();
    while let Some(part) = parts.next() {
        if parts.peek().is_none() {
            current.as_object_mut().unwrap().insert(part.to_string(), content);
            return;
        }
        current = current.as_object_mut().unwrap().entry(part.to_string()).or_insert_with(|| json!({}));
    }
}

fn test_public_btc_anchor(profile: &str, seed: u8) -> Value {
    match profile {
        EXPECTED_BTC_TX_COMMITMENT_PROFILE => json!({
            "kind": "btc_transaction_commitment",
            "anchor_source": test_anchor_source(profile),
            "btc_txid": test_hex32(seed),
            "btc_wtxid": test_hex32(seed.wrapping_add(1)),
            "btc_output_index": 0,
            "btc_amount_sats": 50_000,
            "ckb_btc_commitment_hash": test_hex32(seed.wrapping_add(2)),
        }),
        EXPECTED_BTC_UTXO_SEAL_PROFILE => json!({
            "kind": "btc_utxo_spend",
            "anchor_source": test_anchor_source(profile),
            "sealed_btc_txid": test_hex32(seed),
            "sealed_btc_vout_index": 1,
            "sealed_btc_amount_sats": 75_000,
            "script_pubkey_hash": test_hex32(seed.wrapping_add(1)),
            "btc_txid": test_hex32(seed.wrapping_add(2)),
            "btc_wtxid": test_hex32(seed.wrapping_add(3)),
            "spend_input_index": 0,
            "ckb_btc_commitment_hash": test_hex32(seed.wrapping_add(4)),
            "sealed_utxo_commitment_hash": test_hex32(seed.wrapping_add(5)),
        }),
        EXPECTED_DUAL_SEAL_PROFILE => json!({
            "kind": "dual_seal_btc_closure",
            "anchor_source": test_anchor_source(profile),
            "sealed_btc_txid": test_hex32(seed),
            "sealed_btc_vout_index": 1,
            "sealed_btc_amount_sats": 85_000,
            "script_pubkey_hash": test_hex32(seed.wrapping_add(1)),
            "btc_txid": test_hex32(seed.wrapping_add(2)),
            "btc_wtxid": test_hex32(seed.wrapping_add(3)),
            "spend_input_index": 0,
            "ckb_btc_commitment_hash": test_hex32(seed.wrapping_add(4)),
            "sealed_utxo_commitment_hash": test_hex32(seed.wrapping_add(5)),
        }),
        _ => Value::Null,
    }
}

fn write_expected_live_fixture_reports(repo_root: &Path) -> BTreeMap<String, Value> {
    let mut reports = BTreeMap::<String, Value>::new();
    for (index, expected) in EXPECTED_PROFILE_OPERATOR_FIXTURES.iter().enumerate() {
        let report = reports.entry(expected.live_report.to_string()).or_insert_with(|| json!({"status": "passed"}));
        set_json_pointer_string(report, expected.live_tx_hash_pointer, test_hex32(index as u8 + 1));
        if let Some(anchor_pointer) = expected_public_btc_anchor_pointer(expected.profile) {
            let anchor = test_public_btc_anchor(expected.profile, index as u8 + 0x40);
            let ckb_btc_commitment_hash = json_pointer_str(&anchor, "/ckb_btc_commitment_hash").unwrap().to_string();
            set_json_pointer_value(report, anchor_pointer, anchor);
            if let Some(commitment_pointer) = expected_public_btc_commitment_hash_pointer(expected.profile) {
                set_json_pointer_string(report, commitment_pointer, ckb_btc_commitment_hash);
            }
        }
    }
    for (path, report) in &reports {
        let absolute = repo_root.join(path);
        std::fs::create_dir_all(absolute.parent().unwrap()).unwrap();
        std::fs::write(&absolute, serde_json::to_string(report).unwrap()).unwrap();
    }
    let fiber = json!({"workflow_coverage": {"all_required_workflows_executed_passed": true}});
    let fiber_path = repo_root.join(FIBER_NODE_EXPERIMENTS);
    std::fs::create_dir_all(fiber_path.parent().unwrap()).unwrap();
    std::fs::write(&fiber_path, serde_json::to_string(&fiber).unwrap()).unwrap();
    reports
}

fn operator_fixture_report(repo_root: &Path) -> Value {
    let mut cases = Vec::new();
    let mut profiles = BTreeSet::new();
    for expected in EXPECTED_PROFILE_OPERATOR_FIXTURES {
        profiles.insert(expected.profile);
        let live_report = json_load_path(repo_root, &repo_root.join(expected.live_report)).unwrap();
        let live_report_hash = novaseal_profile_operator_report_hash(expected.live_report, &live_report);
        let live_tx_hash = json_pointer_str(&live_report, expected.live_tx_hash_pointer).unwrap();
        let fiber_report_hash = expected.fiber_report.map(|path| {
            let report = json_load_path(repo_root, &repo_root.join(path)).unwrap();
            novaseal_profile_operator_report_hash(path, &report)
        });
        let public_btc_anchor = expected_public_btc_anchor_pointer(expected.profile)
            .and_then(|pointer| live_report.pointer(pointer))
            .cloned()
            .unwrap_or(Value::Null);
        let hash = test_hex32(0xaa);
        cases.push(json!({
            "profile": expected.profile,
            "action": expected.action,
            "fixture": expected.fixture,
            "status": "passed",
            "checks": {"fixture_expected_accepted": true},
            "signers": expected.signers,
            "signed_type": "NovaFixtureSignedIntentV0",
            "signed_intent_hash": hash,
            "bip340_message_hash": hash,
            "signed_intent_body_hex": "0x00",
            "signed_intent_hash_preimage_hex": "0x01",
            "witness_shape_hash": test_hex32(0x11),
            "tx_skeleton_hash": test_hex32(0x12),
            "fixture_hash": test_hex32(0x13),
            "source_tree_hash": test_hex32(0x14),
            "schema_set_hash": test_hex32(0x15),
            "proof_matrix_hash": test_hex32(0x16),
            "live_report_hash": live_report_hash,
            "fiber_report_hash": fiber_report_hash,
            "live_devnet_tx_hash": live_tx_hash,
            "public_btc_anchor": public_btc_anchor.clone(),
            "wallet_display": {
                "profile": expected.profile,
                "action": expected.action,
                "live_devnet_tx_hash": live_tx_hash,
                "public_btc_anchor": public_btc_anchor,
            },
        }));
    }
    json!({
        "schema": "novaseal-profile-operator-fixtures-v0.1",
        "status": "passed",
        "summary": {
            "total": EXPECTED_PROFILE_OPERATOR_FIXTURES.len(),
            "matched": EXPECTED_PROFILE_OPERATOR_FIXTURES.len(),
            "profile_count": profiles.len(),
        },
        "cases": cases,
    })
}

fn service_builder_report(operator_fixtures: &Value) -> Value {
    let operator_cases = operator_fixtures.get("cases").and_then(Value::as_array).unwrap();
    let mut cases = Vec::new();
    let mut profiles = BTreeSet::new();
    for operator_case in operator_cases {
        let profile = json_pointer_str(operator_case, "/profile").unwrap();
        let action = json_pointer_str(operator_case, "/action").unwrap();
        let fixture = json_pointer_str(operator_case, "/fixture").unwrap();
        let signers = json_array_strings(operator_case, "/signers");
        profiles.insert(profile.to_string());
        let operator_fixture_hash = novaseal_service_builder_report_hash("operator_case", operator_case);
        let signed_hash = json_pointer_str(operator_case, "/signed_intent_hash").unwrap();
        let witness_hash = json_pointer_str(operator_case, "/witness_shape_hash").unwrap();
        let public_btc_anchor = operator_case.pointer("/public_btc_anchor").cloned().unwrap_or(Value::Null);
        cases.push(json!({
            "profile": profile,
            "action": action,
            "fixture": fixture,
            "status": "passed",
            "checks": {"operator_case_passed": true},
            "builder_name": "novaseal-profile-service-builder-v0",
            "operator_fixture_hash": operator_fixture_hash,
            "signers": signers,
            "request": {
                "schema": "novaseal-service-builder-request-v0.1",
                "builder_name": "novaseal-profile-service-builder-v0",
                "profile": profile,
                "action": action,
                "signers": signers,
                "idempotency_key": test_hex32(0x20),
                "operator_fixture_hash": operator_fixture_hash,
                "required_profile_inputs": {
                    "source_tree_hash": json_pointer_str(operator_case, "/source_tree_hash"),
                    "schema_set_hash": json_pointer_str(operator_case, "/schema_set_hash"),
                    "proof_matrix_hash": json_pointer_str(operator_case, "/proof_matrix_hash"),
                    "fixture_hash": json_pointer_str(operator_case, "/fixture_hash"),
                },
                "required_live_inputs": {
                    "live_report_hash": json_pointer_str(operator_case, "/live_report_hash"),
                    "live_devnet_tx_hash": json_pointer_str(operator_case, "/live_devnet_tx_hash"),
                    "fiber_report_hash": json_pointer_str(operator_case, "/fiber_report_hash"),
                    "public_btc_anchor": public_btc_anchor.clone(),
                },
                "production_external_inputs": ["public_shared_cell_dep_attestation"],
            },
            "response": {
                "schema": "novaseal-service-builder-response-v0.1",
                "profile": profile,
                "action": action,
                "service_queue_key": test_hex32(0x21),
                "tx_skeleton_hash": test_hex32(0x22),
                "witness_shape_hash": witness_hash,
                "signed_intent_hash": signed_hash,
                "bip340_message_hash": signed_hash,
                "receipt_binding_hash": test_hex32(0x23),
                "builder_trace_hash": test_hex32(0x24),
            },
            "tx_skeleton": {
                "schema": "novaseal-service-builder-tx-skeleton-v0.1",
                "operator_fixture_hash": operator_fixture_hash,
                "public_btc_anchor": public_btc_anchor,
            },
        }));
    }
    json!({
        "schema": "novaseal-service-builder-fixtures-v0.1",
        "status": "passed",
        "builder_name": "novaseal-profile-service-builder-v0",
        "source_operator_fixture_report_hash": novaseal_service_builder_report_hash("operator_report", operator_fixtures),
        "summary": {
            "total": EXPECTED_PROFILE_OPERATOR_FIXTURES.len(),
            "matched": EXPECTED_PROFILE_OPERATOR_FIXTURES.len(),
            "profile_count": profiles.len(),
        },
        "cases": cases,
    })
}

fn wallet_lock_alignment_report(legacy_domain_hash_visible: bool) -> Value {
    let fixtures = (0..11)
        .map(|index| {
            json!({
                "fixture": format!("fixture_{index}.json"),
                "resolved_intent_size_bytes": 254,
                "canonical_wallet_message32": test_hex32(0x30),
                "current_lock_message32": test_hex32(0x30),
                "current_lock_message_rule": "hash_blake2b_packed(NovaSealSignedIntentV0 { core, expected_receipt_hash })",
                "canonical_wallet_message_rule": "signed_intent_hash_after_resolved_receipt",
                "canonical_vs_current_lock_digest_match": true,
                "canonical_wallet_positive": {
                    "self_verified": true,
                },
                "current_lock_compat_positive": {
                    "self_verified": true,
                },
                "cross_check": {
                    "canonical_signature_accepts_current_lock_digest": true,
                    "current_lock_signature_accepts_canonical_digest": true,
                },
            })
        })
        .collect::<Vec<_>>();
    json!({
        "schema": "novaseal-wallet-signing-alignment-v0.2",
        "classification": "wallet_signing_vectors_and_lock_digest_alignment_probe",
        "source_digest_model": {
            "all_required_snippets_present": !legacy_domain_hash_visible,
            "state_type_uses_packed_signed_intent_hash": true,
            "state_type_verifier_uses_signed_intent_hash": true,
            "package_lock_uses_packed_digest": true,
            "standalone_lock_uses_packed_digest": true,
            "legacy_domain_hash_visible": legacy_domain_hash_visible,
        },
        "summary": {
            "fixtures": 11,
            "canonical_wallet_vectors_self_verified": 11,
            "current_lock_compat_vectors_self_verified": 11,
            "current_lock_digest_matches_canonical": 11,
            "current_lock_digest_mismatches": 0,
            "canonical_wallet_signatures_accepted_by_current_lock_digest": 11,
            "current_lock_signatures_accepted_by_canonical_wallet_digest": 11,
            "wallet_lock_alignment_ready": true,
            "production_wallet_ready": true,
        },
        "fixtures": fixtures,
    })
}
