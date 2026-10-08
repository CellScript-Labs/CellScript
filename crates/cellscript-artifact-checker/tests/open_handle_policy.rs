use cellscript_artifact_checker::open_handle_policy::*;

fn header(count: u8) -> PolicyHeader {
    PolicyHeader {
        class: HandleClass::Script,
        role: ScriptRole::Type,
        mode: SelectionMode::Compatible,
        member_count: count,
        sequence: 10,
        minimum_admission_sequence: 3,
        required_interface: [40; 32],
        exact_receipt: [0; 32],
        network_genesis: [41; 32],
        target_profile: [42; 32],
        runtime_abi: [43; 32],
    }
}
fn member(id: u8) -> PolicyMember {
    PolicyMember {
        status: MemberStatus::Active,
        hash_type: CodeHashType::Data2,
        admission_sequence: 5,
        deployment_sequence: 0,
        receipt: [id; 32],
        interface: [50; 32],
        artifact: [id + 64; 32],
        script: [id + 96; 32],
        code_hash: [id + 64; 32],
        code_tx_hash: [id + 128; 32],
        deployment_line: [0; 32],
        history_tip: [0; 32],
        code_output_index: id as u32,
    }
}

#[test]
fn every_cardinality_and_leaf_has_a_canonical_order_independent_membership() {
    for count in 1..=MAX_MEMBERS as u8 {
        let members = (1..=count).map(member).collect::<Vec<_>>();
        let original = AuthorizationSet::new(header(count), &members).unwrap();
        let mut reversed = members.clone();
        reversed.reverse();
        let reordered = AuthorizationSet::new(header(count), &reversed).unwrap();
        assert_eq!(original.root(), reordered.root());
        for (index, expected) in members.iter().enumerate() {
            let witness = original.selection(&expected.receipt).unwrap();
            assert_eq!(witness, reordered.selection(&expected.receipt).unwrap());
            let membership = verify_selection(&original.root(), &witness).unwrap();
            assert_eq!(membership.root(), original.root());
            assert_eq!(membership.index() as usize, index);
            assert_eq!(membership.member(), expected);
            assert_eq!(membership.header(), original.header());
        }
    }
}

#[test]
fn full_selection_is_bound_including_header_member_position_and_every_sibling() {
    let set = AuthorizationSet::new(header(3), &[member(1), member(2), member(3)]).unwrap();
    let witness = set.selection(&member(2).receipt).unwrap();
    for index in 0..SELECTION_BYTES {
        let mut changed = witness;
        changed[index] ^= 1;
        assert!(verify_selection(&set.root(), &changed).is_err(), "unbound byte {index}");
    }
    for length in 0..SELECTION_BYTES {
        assert_eq!(verify_selection(&set.root(), &witness[..length]).unwrap_err(), PolicyError::Length);
    }
    let mut oversized = witness.to_vec();
    oversized.push(0);
    assert_eq!(verify_selection(&set.root(), &oversized).unwrap_err(), PolicyError::Length);
    assert_eq!(verify_selection(&[0; 32], &witness).unwrap_err(), PolicyError::Root);
    let mut bad_index = witness;
    bad_index[488] = 3;
    assert_eq!(verify_selection(&set.root(), &bad_index).unwrap_err(), PolicyError::Index);
}

#[test]
fn immutable_snapshots_do_not_claim_registry_freshness_and_new_roots_reject_old_proofs() {
    let old = AuthorizationSet::new(header(1), &[member(1)]).unwrap();
    let old_witness = old.selection(&member(1).receipt).unwrap();
    let mut yanked = member(1);
    yanked.status = MemberStatus::Yanked;
    let mut next_header = header(1);
    next_header.sequence += 1;
    let next = AuthorizationSet::new(next_header.clone(), &[yanked.clone()]).unwrap();
    assert_ne!(old.root(), next.root());
    // A later off-chain publication cannot mutate the old committed snapshot.
    verify_selection(&old.root(), &old_witness).unwrap();
    assert_eq!(verify_selection(&next.root(), &old_witness).unwrap_err(), PolicyError::Root);
    assert_eq!(verify_selection(&next.root(), &next.selection(&yanked.receipt).unwrap()).unwrap_err(), PolicyError::Inactive);
    next_header.minimum_admission_sequence = 6;
    let stale = AuthorizationSet::new(next_header, &[member(1)]).unwrap();
    assert_eq!(verify_selection(&stale.root(), &stale.selection(&member(1).receipt).unwrap()).unwrap_err(), PolicyError::Sequence);
    let mut future = member(1);
    future.admission_sequence = 11;
    assert!(matches!(AuthorizationSet::new(header(1), &[future]), Err(PolicyError::Sequence)));
}

#[test]
fn exact_mode_filters_receipts_without_inventing_compatibility() {
    let mut exact = header(2);
    exact.mode = SelectionMode::Exact;
    exact.exact_receipt = member(2).receipt;
    let set = AuthorizationSet::new(exact, &[member(1), member(2)]).unwrap();
    verify_selection(&set.root(), &set.selection(&member(2).receipt).unwrap()).unwrap();
    assert_eq!(verify_selection(&set.root(), &set.selection(&member(1).receipt).unwrap()).unwrap_err(), PolicyError::ExactReceipt);
    let mut absent = header(1);
    absent.mode = SelectionMode::Exact;
    absent.exact_receipt = [99; 32];
    assert!(matches!(AuthorizationSet::new(absent, &[member(1)]), Err(PolicyError::ExactReceipt)));
    // Compatible mode merely authorizes selection from the committed records;
    // the artifact/interface admission operation is a separate prerequisite.
    let set = AuthorizationSet::new(header(2), &[member(1), member(2)]).unwrap();
    for member in set.members() {
        verify_selection(&set.root(), &set.selection(&member.receipt).unwrap()).unwrap();
    }
}

#[test]
fn membership_result_retains_its_authorizing_root_when_the_selected_member_is_identical() {
    let selected = member(1);
    let other = member(2);
    let first = AuthorizationSet::new(header(2), &[selected.clone(), other.clone()]).unwrap();
    let mut changed = other;
    changed.status = MemberStatus::Yanked;
    let second = AuthorizationSet::new(header(2), &[selected.clone(), changed]).unwrap();
    let a = verify_selection(&first.root(), &first.selection(&selected.receipt).unwrap()).unwrap();
    let b = verify_selection(&second.root(), &second.selection(&selected.receipt).unwrap()).unwrap();
    assert_eq!(a.member(), b.member());
    assert_eq!(a.header(), b.header());
    assert_eq!(a.index(), b.index());
    assert_eq!(a.root(), first.root());
    assert_eq!(b.root(), second.root());
    assert_ne!(a.root(), b.root());
}

#[test]
fn type_hash_history_is_retained_and_conflicting_coordinates_are_rejected() {
    let mut first = member(1);
    first.hash_type = CodeHashType::Type;
    first.code_hash = [80; 32];
    first.deployment_line = [81; 32];
    first.history_tip = [82; 32];
    let mut next = first.clone();
    next.receipt = [2; 32];
    next.artifact = [83; 32];
    next.code_tx_hash = [84; 32];
    next.history_tip = [85; 32];
    next.deployment_sequence = 1;
    let set = AuthorizationSet::new(header(2), &[first.clone(), next.clone()]).unwrap();
    assert_eq!(verify_selection(&set.root(), &set.selection(&next.receipt).unwrap()).unwrap().member(), &next);
    next.deployment_sequence = 0;
    assert!(matches!(AuthorizationSet::new(header(2), &[first, next]), Err(PolicyError::Conflict)));
    let first = member(1);
    let mut conflict = member(2);
    conflict.code_tx_hash = first.code_tx_hash;
    conflict.code_output_index = first.code_output_index;
    assert!(matches!(AuthorizationSet::new(header(2), &[first.clone(), conflict]), Err(PolicyError::Conflict)));
    let mut duplicate = first.clone();
    duplicate.script = [55; 32];
    assert!(matches!(AuthorizationSet::new(header(2), &[first, duplicate]), Err(PolicyError::Duplicate)));
}

#[test]
fn record_codecs_reject_noncanonical_tags_reserved_bytes_and_lengths() {
    let valid_header = header(1).encode().unwrap();
    let valid_member = member(1).encode().unwrap();
    assert_eq!(PolicyHeader::decode(&valid_header).unwrap(), header(1));
    assert_eq!(PolicyMember::decode(&valid_member).unwrap(), member(1));
    for length in 0..HEADER_BYTES {
        assert_eq!(PolicyHeader::decode(&valid_header[..length]).unwrap_err(), PolicyError::Length);
    }
    for length in 0..MEMBER_BYTES {
        assert_eq!(PolicyMember::decode(&valid_member[..length]).unwrap_err(), PolicyError::Length);
    }
    for offset in [8, 9, 10] {
        let mut bytes = valid_header;
        bytes[offset] = 255;
        assert_eq!(PolicyHeader::decode(&bytes).unwrap_err(), PolicyError::Tag);
    }
    for offset in [8, 9] {
        let mut bytes = valid_member;
        bytes[offset] = 255;
        assert_eq!(PolicyMember::decode(&bytes).unwrap_err(), PolicyError::Tag);
    }
    for offset in 10..16 {
        let mut bytes = valid_member;
        bytes[offset] = 1;
        assert_eq!(PolicyMember::decode(&bytes).unwrap_err(), PolicyError::Reserved);
    }
    for count in [0, 33, 255] {
        let mut bytes = valid_header;
        bytes[11] = count;
        assert_eq!(PolicyHeader::decode(&bytes).unwrap_err(), PolicyError::Count);
    }
    assert!(matches!(AuthorizationSet::new(header(2), &[member(1)]), Err(PolicyError::Count)));
    let mut wrong_data = member(1);
    wrong_data.code_hash = [19; 32];
    assert_eq!(wrong_data.encode().unwrap_err(), PolicyError::Member);
    let mut missing_history = member(1);
    missing_history.hash_type = CodeHashType::Type;
    assert_eq!(missing_history.encode().unwrap_err(), PolicyError::Member);
    for role in [ScriptRole::Lock, ScriptRole::Type, ScriptRole::SpawnedVerifier] {
        let mut h = header(1);
        h.role = role;
        h.class = if role == ScriptRole::SpawnedVerifier { HandleClass::Verifier } else { HandleClass::Script };
        let set = AuthorizationSet::new(h.clone(), &[member(1)]).unwrap();
        verify_selection(&set.root(), &set.selection(&member(1).receipt).unwrap()).unwrap();
        h.class = if h.class == HandleClass::Script { HandleClass::Verifier } else { HandleClass::Script };
        assert_eq!(h.encode().unwrap_err(), PolicyError::Header);
    }
    let mut empty_exact = header(1);
    empty_exact.mode = SelectionMode::Exact;
    assert_eq!(empty_exact.encode().unwrap_err(), PolicyError::Header);
    let mut invalid_compatible = header(1);
    invalid_compatible.exact_receipt = [1; 32];
    assert_eq!(invalid_compatible.encode().unwrap_err(), PolicyError::Header);
}

#[test]
fn sequence_and_outpoint_integer_extremes_are_lossless_and_policy_fields_are_immutable() {
    let mut h = header(1);
    h.sequence = u64::MAX;
    h.minimum_admission_sequence = u64::MAX;
    let mut m = member(1);
    m.admission_sequence = u64::MAX;
    m.code_output_index = u32::MAX;
    for hash_type in [CodeHashType::Data, CodeHashType::Data1, CodeHashType::Data2] {
        m.hash_type = hash_type;
        let set = AuthorizationSet::new(h.clone(), &[m.clone()]).unwrap();
        let root = set.root();
        let proof = set.selection(&m.receipt).unwrap();
        assert_eq!(verify_selection(&root, &proof).unwrap().member(), &m);
        assert_eq!(PolicyHeader::decode(&h.encode().unwrap()).unwrap(), h);
        assert_eq!(PolicyMember::decode(&m.encode().unwrap()).unwrap(), m);
    }
    let original = AuthorizationSet::new(h.clone(), &[m.clone()]).unwrap();
    let proof = original.selection(&m.receipt).unwrap();
    h.sequence = 0;
    m.receipt = [99; 32];
    assert_eq!(h.sequence, 0);
    assert_eq!(m.receipt, [99; 32]);
    verify_selection(&original.root(), &proof).unwrap();
    assert_eq!(original.header().sequence, u64::MAX);
    assert_eq!(original.members()[0].receipt, [1; 32]);
}

#[test]
fn ccc_molecule_and_ckb_hash_vectors_agree_byte_for_byte() {
    fn bytes(value: &serde_json::Value) -> Vec<u8> {
        let hex = value.as_str().unwrap().strip_prefix("0x").unwrap();
        assert_eq!(hex.len() % 2, 0);
        hex.as_bytes().chunks_exact(2).map(|pair| u8::from_str_radix(std::str::from_utf8(pair).unwrap(), 16).unwrap()).collect()
    }
    let vectors: serde_json::Value = serde_json::from_str(include_str!("fixtures/open_handle_policy_vectors.json")).unwrap();
    assert_eq!(vectors["schema"], "cellscript-open-handle-policy-wire-vectors-v1");
    assert_eq!(
        vectors["molecule_schema_hash"],
        format!(
            "0x{}",
            cellscript_artifact_checker::hex_encode(&cellscript_artifact_checker::ckb_blake2b256(include_bytes!(
                "../src/open_handle_policy.mol"
            )))
        )
    );
    let cases = vectors["cases"].as_array().unwrap();
    assert_eq!(cases.iter().map(|case| case["count"].as_u64().unwrap()).collect::<Vec<_>>(), [1, 3, 32]);
    for case in cases {
        let count = case["count"].as_u64().unwrap() as u8;
        assert_eq!(case["members"].as_array().unwrap().len(), count as usize);
        assert_eq!(case["selections"].as_array().unwrap().len(), count as usize);
        let expected_header = header(count);
        assert_eq!(expected_header.encode().unwrap().as_slice(), bytes(&case["header"]));
        assert_eq!(PolicyHeader::decode(&bytes(&case["header"])).unwrap(), expected_header);
        let members = (1..=count).map(member).collect::<Vec<_>>();
        for (value, vector) in members.iter().zip(case["members"].as_array().unwrap()) {
            assert_eq!(value.encode().unwrap().as_slice(), bytes(vector));
            assert_eq!(PolicyMember::decode(&bytes(vector)).unwrap(), *value);
        }
        let set = AuthorizationSet::new(expected_header, &members).unwrap();
        let expected_root: Hash = bytes(&case["root"]).try_into().unwrap();
        assert_eq!(set.root(), expected_root);
        for (value, vector) in members.iter().zip(case["selections"].as_array().unwrap()) {
            let encoded = set.selection(&value.receipt).unwrap();
            assert_eq!(encoded.as_slice(), bytes(vector));
            assert_eq!(verify_selection(&expected_root, &bytes(vector)).unwrap().member(), value);
        }
    }
}
