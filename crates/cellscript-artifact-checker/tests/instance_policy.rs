use cellscript_artifact_checker::ckb_blake2b256;
use cellscript_artifact_checker::instance_policy::{
    self as instance, ArgsConstraint, ConsumerArgs, InstanceError, InstanceMode, InstancePolicy,
};
use cellscript_artifact_checker::open_handle_policy::{
    self as implementation, AuthorizationSet, CodeHashType, HandleClass, MemberStatus, PolicyHeader, PolicyMember, ScriptRole,
    SelectionMode,
};

#[test]
fn independent_ccc_molecule_instance_vectors_agree_with_policy_and_selection() {
    fn bytes(value: &serde_json::Value) -> Vec<u8> {
        let value = value.as_str().unwrap().strip_prefix("0x").unwrap();
        assert_eq!(value.len() % 2, 0);
        value.as_bytes().chunks_exact(2).map(|pair| u8::from_str_radix(std::str::from_utf8(pair).unwrap(), 16).unwrap()).collect()
    }
    let vectors: serde_json::Value = serde_json::from_str(include_str!("fixtures/instance_policy_vectors.json")).unwrap();
    assert_eq!(vectors["schema"], "cellscript-instance-policy-wire-vectors-v1");
    assert_eq!(bytes(&vectors["molecule_schema_hash"]), ckb_blake2b256(include_bytes!("../src/instance_policy.mol")));
    let cases = vectors["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 14);
    for case in cases {
        let args = bytes(&case["args"]);
        let config = bytes(&case["config"]);
        let implementation_root = bytes(&case["implementation_root"]).try_into().unwrap();
        let constraints = case["rules"]
            .as_array()
            .unwrap()
            .iter()
            .map(|rule| {
                let offset = rule["offset"].as_u64().unwrap() as u16;
                let length = rule["length"].as_u64().unwrap() as u16;
                let value = bytes(&rule["value"]);
                match rule["tag"].as_u64().unwrap() {
                    0 => ArgsConstraint::Bytes { offset, value: value[..length as usize].to_vec() },
                    1 => ArgsConstraint::Unsigned {
                        offset,
                        width: length,
                        minimum: u64::from_le_bytes(value[..8].try_into().unwrap()),
                        maximum: u64::from_le_bytes(value[8..16].try_into().unwrap()),
                    },
                    2 => ArgsConstraint::Config { offset, length, config_offset: rule["configOffset"].as_u64().unwrap() as u16 },
                    3 => ArgsConstraint::Any { offset, length },
                    tag => panic!("unexpected reference tag {tag}"),
                }
            })
            .collect();
        let policy = InstancePolicy {
            implementation_root,
            mode: if case["instance_mode"] == 0 { InstanceMode::Exact } else { InstanceMode::Constrained },
            args_length: args.len() as u16,
            config_length: config.len() as u16,
            constraints,
        };
        assert_eq!(policy.encode().unwrap().as_slice(), bytes(&case["policy"]), "{}", case["name"]);
        assert_eq!(InstancePolicy::decode(&bytes(&case["policy"])).unwrap(), policy);
        assert_eq!(policy.root().unwrap().as_slice(), bytes(&case["root"]));
        assert_eq!(policy.args_schema().unwrap().as_slice(), bytes(&case["schema"]));
        policy.check_args(&args, &config).unwrap();
        let context =
            ConsumerArgs { policy_root: policy.root().unwrap(), subject: bytes(&case["subject"]).try_into().unwrap(), config };
        assert_eq!(context.encode().unwrap().as_slice(), bytes(&case["consumer"]));
        assert_eq!(ConsumerArgs::decode(&bytes(&case["consumer"])).unwrap(), context);
        let old = bytes(&case["implementation_selection"]);
        let encoded = instance::encode_selection(&policy, &old, &args).unwrap();
        assert_eq!(encoded.as_slice(), bytes(&case["selection"]));
        let checked = instance::verify_selection(&bytes(&case["consumer"]), &bytes(&case["selection"])).unwrap();
        assert_eq!(checked.script(), bytes(&case["script"]));
        assert_eq!(checked.script_hash(), context.subject);
        assert_eq!(
            checked.implementation().header().mode,
            if case["implementation_mode"] == 0 { SelectionMode::Exact } else { SelectionMode::Compatible }
        );
        implementation::verify_selection(&implementation_root, &old).unwrap();
        assert!(implementation::verify_selection(&implementation_root, &encoded).is_err());
        assert!(instance::verify_selection(&context.encode().unwrap(), &old).is_err());
    }
}

fn set(mode: SelectionMode, role: ScriptRole, args: &[u8], status: MemberStatus, floor: u64) -> AuthorizationSet {
    let member = PolicyMember {
        status,
        hash_type: CodeHashType::Data2,
        admission_sequence: 5,
        deployment_sequence: 0,
        receipt: [1; 32],
        interface: [2; 32],
        artifact: [3; 32],
        script: ckb_blake2b256(&instance::instance_script(&[3; 32], CodeHashType::Data2, args).unwrap()),
        code_hash: [3; 32],
        code_tx_hash: [4; 32],
        deployment_line: [0; 32],
        history_tip: [0; 32],
        code_output_index: 7,
    };
    AuthorizationSet::new(
        PolicyHeader {
            class: if role == ScriptRole::SpawnedVerifier { HandleClass::Verifier } else { HandleClass::Script },
            role,
            mode,
            member_count: 1,
            sequence: 10,
            minimum_admission_sequence: floor,
            required_interface: [5; 32],
            exact_receipt: if mode == SelectionMode::Exact { member.receipt } else { [0; 32] },
            network_genesis: [6; 32],
            target_profile: [7; 32],
            runtime_abi: [8; 32],
        },
        &[member],
    )
    .unwrap()
}

fn make_policy(set: &AuthorizationSet, mode: InstanceMode, args: &[u8]) -> InstancePolicy {
    InstancePolicy {
        implementation_root: set.root(),
        mode,
        args_length: args.len() as u16,
        config_length: 0,
        constraints: if mode == InstanceMode::Exact {
            args.chunks(32)
                .enumerate()
                .map(|(index, bytes)| ArgsConstraint::Bytes { offset: (index * 32) as u16, value: bytes.to_vec() })
                .collect()
        } else if args.is_empty() {
            Vec::new()
        } else {
            vec![ArgsConstraint::Any { offset: 0, length: args.len() as u16 }]
        },
    }
}

fn consumer(policy: &InstancePolicy, args: &[u8], config: Vec<u8>) -> ConsumerArgs {
    ConsumerArgs {
        policy_root: policy.root().unwrap(),
        subject: ckb_blake2b256(&instance::instance_script(&[3; 32], CodeHashType::Data2, args).unwrap()),
        config,
    }
}

#[test]
fn implementation_and_instance_modes_are_separate_and_old_witnesses_keep_their_meaning() {
    for selection_mode in [SelectionMode::Exact, SelectionMode::Compatible] {
        for role in [ScriptRole::Lock, ScriptRole::Type, ScriptRole::SpawnedVerifier] {
            for instance_mode in [InstanceMode::Exact, InstanceMode::Constrained] {
                let baseline = [9; 32];
                let set = set(selection_mode, role, &baseline, MemberStatus::Active, 3);
                let old = set.selection(&[1; 32]).unwrap();
                let policy = make_policy(&set, instance_mode, &baseline);
                let args = if instance_mode == InstanceMode::Exact { baseline } else { [10; 32] };
                let witness = instance::encode_selection(&policy, &old, &args).unwrap();
                let context = consumer(&policy, &args, vec![]).encode().unwrap();
                let checked = instance::verify_selection(&context, &witness);
                if role == ScriptRole::SpawnedVerifier && instance_mode == InstanceMode::Constrained {
                    assert_eq!(checked.unwrap_err(), InstanceError::Mode);
                } else {
                    let checked = checked.unwrap();
                    assert_eq!(checked.implementation().header().mode, selection_mode);
                    assert_eq!(checked.implementation().member().script, set.members()[0].script);
                    assert_eq!(&checked.script()[53..], args);
                    assert_eq!(checked.policy().mode, instance_mode);
                }
                let old_checked = implementation::verify_selection(&set.root(), &old).unwrap();
                assert_eq!(old_checked.member().script, set.members()[0].script);
                assert!(implementation::verify_selection(&set.root(), &witness).is_err());
                assert!(instance::verify_selection(&context, &old).is_err());
                assert!(instance::verify_selection(&set.root(), &witness).is_err());
            }
        }
    }
}

#[test]
fn configured_instances_share_implementation_but_cannot_substitute_for_an_authorized_subject() {
    // Non-financial example: a document observer accepts an issuer bound
    // to its own committed config, an expiry interval and an explicit nonce.
    let set = set(SelectionMode::Compatible, ScriptRole::Type, &[0; 49], MemberStatus::Active, 3);
    let policy = InstancePolicy {
        implementation_root: set.root(),
        mode: InstanceMode::Constrained,
        args_length: 49,
        config_length: 32,
        constraints: vec![
            ArgsConstraint::Bytes { offset: 0, value: vec![1] },
            ArgsConstraint::Config { offset: 1, length: 32, config_offset: 0 },
            ArgsConstraint::Unsigned { offset: 33, width: 8, minimum: 100, maximum: 200 },
            ArgsConstraint::Any { offset: 41, length: 8 },
        ],
    };
    let mut args_a = vec![1];
    args_a.extend_from_slice(&[11; 32]);
    args_a.extend_from_slice(&150u64.to_le_bytes());
    args_a.extend_from_slice(&1u64.to_le_bytes());
    let mut args_b = args_a.clone();
    args_b[41..].copy_from_slice(&2u64.to_le_bytes());
    let implementation = set.selection(&[1; 32]).unwrap();
    let witness_a = instance::encode_selection(&policy, &implementation, &args_a).unwrap();
    let witness_b = instance::encode_selection(&policy, &implementation, &args_b).unwrap();
    let consumer_a = consumer(&policy, &args_a, vec![11; 32]).encode().unwrap();
    let consumer_b = consumer(&policy, &args_b, vec![11; 32]).encode().unwrap();
    let a = instance::verify_selection(&consumer_a, &witness_a).unwrap();
    let b = instance::verify_selection(&consumer_b, &witness_b).unwrap();
    assert_eq!(a.implementation().member(), b.implementation().member());
    assert_ne!(a.script_hash(), b.script_hash());
    assert_eq!(instance::verify_selection(&consumer_a, &witness_b).unwrap_err(), InstanceError::Subject);
    assert_eq!(instance::verify_selection(&consumer_b, &witness_a).unwrap_err(), InstanceError::Subject);

    for offset in [0, 1, 33] {
        let mut invalid = args_a.clone();
        invalid[offset] = 0;
        let witness = instance::encode_selection(&policy, &implementation, &invalid).unwrap();
        // Even granting a new subject identity cannot bypass the old policy.
        let context = consumer(&policy, &invalid, vec![11; 32]).encode().unwrap();
        assert_eq!(instance::verify_selection(&context, &witness).unwrap_err(), InstanceError::Constraint);
    }
    for value in [100u64, 200] {
        let mut args = args_a.clone();
        args[33..41].copy_from_slice(&value.to_le_bytes());
        policy.check_args(&args, &[11; 32]).unwrap();
    }
    for value in [99u64, 201, u64::MAX] {
        let mut args = args_a.clone();
        args[33..41].copy_from_slice(&value.to_le_bytes());
        assert_eq!(policy.check_args(&args, &[11; 32]), Err(InstanceError::Constraint));
    }
}

#[test]
fn selection_mutations_padding_lengths_and_policy_widening_fail_closed() {
    let args = [9; 32];
    let set = set(SelectionMode::Compatible, ScriptRole::Type, &args, MemberStatus::Active, 3);
    let policy = make_policy(&set, InstanceMode::Exact, &args);
    let old = set.selection(&[1; 32]).unwrap();
    let witness = instance::encode_selection(&policy, &old, &args).unwrap();
    let context = consumer(&policy, &args, vec![]).encode().unwrap();
    for offset in 0..witness.len() {
        let mut changed = witness;
        changed[offset] ^= 1;
        assert!(instance::verify_selection(&context, &changed).is_err(), "unbound selection byte {offset}");
    }
    for offset in 0..context.len() {
        let mut changed = context;
        changed[offset] ^= 1;
        assert!(instance::verify_selection(&changed, &witness).is_err(), "unbound consumer byte {offset}");
    }
    for length in 0..witness.len() {
        assert_eq!(instance::verify_selection(&context, &witness[..length]).unwrap_err(), InstanceError::Length);
    }
    let mut over = witness.to_vec();
    over.push(0);
    assert_eq!(instance::verify_selection(&context, &over).unwrap_err(), InstanceError::Length);
    let mut widened = policy.clone();
    widened.mode = InstanceMode::Constrained;
    widened.constraints = vec![ArgsConstraint::Any { offset: 0, length: 32 }];
    let widened_witness = instance::encode_selection(&widened, &old, &args).unwrap();
    assert_ne!(widened.root().unwrap(), policy.root().unwrap());
    assert_eq!(instance::verify_selection(&context, &widened_witness).unwrap_err(), InstanceError::Root);
    assert_eq!(
        instance::verify_selection(&consumer(&widened, &args, vec![]).encode().unwrap(), &witness).unwrap_err(),
        InstanceError::Root
    );
    let different = [10; 32];
    let changed_exact = make_policy(&set, InstanceMode::Exact, &different);
    let changed_witness = instance::encode_selection(&changed_exact, &old, &different).unwrap();
    assert_eq!(
        instance::verify_selection(&consumer(&changed_exact, &different, vec![]).encode().unwrap(), &changed_witness).unwrap_err(),
        InstanceError::Subject
    );
}

#[test]
fn fixed_policy_limits_are_checked_before_traversal_and_all_bytes_are_explicit() {
    for length in [0, instance::MAX_ARGS] {
        let args = vec![19; length];
        let set = set(SelectionMode::Exact, ScriptRole::Type, &args, MemberStatus::Active, 3);
        let policy = make_policy(&set, InstanceMode::Exact, &args);
        assert_eq!(InstancePolicy::decode(&policy.encode().unwrap()).unwrap(), policy);
        let context = consumer(&policy, &args, vec![]).encode().unwrap();
        let witness = instance::encode_selection(&policy, &set.selection(&[1; 32]).unwrap(), &args).unwrap();
        instance::verify_selection(&context, &witness).unwrap();
    }
    let set = set(SelectionMode::Compatible, ScriptRole::Type, &[0; 256], MemberStatus::Active, 3);
    let mut policy = make_policy(&set, InstanceMode::Constrained, &[0; 256]);
    policy.constraints = (0..16).map(|index| ArgsConstraint::Any { offset: index * 16, length: 16 }).collect();
    policy.encode().unwrap();
    policy.constraints.push(ArgsConstraint::Any { offset: 256, length: 1 });
    assert_eq!(policy.encode(), Err(InstanceError::Bounds));
    policy.constraints.pop();
    policy.constraints[1] = ArgsConstraint::Any { offset: 15, length: 16 };
    assert_eq!(policy.encode(), Err(InstanceError::Partition));
    policy.constraints[1] = ArgsConstraint::Any { offset: 17, length: 16 };
    assert_eq!(policy.encode(), Err(InstanceError::Partition));
    policy.constraints = vec![ArgsConstraint::Any { offset: 0, length: 257 }];
    assert_eq!(policy.encode(), Err(InstanceError::Bounds));
    policy.constraints = vec![ArgsConstraint::Any { offset: 0, length: 0 }];
    assert_eq!(policy.encode(), Err(InstanceError::Bounds));
    policy.constraints =
        vec![ArgsConstraint::Config { offset: 0, length: 128, config_offset: 0 }, ArgsConstraint::Any { offset: 128, length: 128 }];
    policy.config_length = 128;
    policy.check_args(&[0; 256], &[0; 128]).unwrap();
    policy.config_length = 127;
    assert_eq!(policy.encode(), Err(InstanceError::Bounds));
    policy.config_length = 129;
    assert_eq!(policy.encode(), Err(InstanceError::Bounds));

    assert_eq!(instance::instance_script(&[3; 32], CodeHashType::Data2, &[0; 257]), Err(InstanceError::Bounds));
    let encoded = consumer(&make_policy(&set, InstanceMode::Exact, &[0; 256]), &[0; 256], vec![0; 128]).encode().unwrap();
    assert_eq!(ConsumerArgs::decode(&encoded).unwrap().config.len(), 128);
}

#[test]
fn scalar_constraint_widths_and_noncanonical_encodings_are_rejected() {
    let set = set(SelectionMode::Compatible, ScriptRole::Type, &[0; 8], MemberStatus::Active, 3);
    for width in [1u16, 2, 4, 8] {
        let maximum = if width == 8 { u64::MAX } else { (1u64 << (width * 8)) - 1 };
        let policy = InstancePolicy {
            implementation_root: set.root(),
            mode: InstanceMode::Constrained,
            args_length: width,
            config_length: 0,
            constraints: vec![ArgsConstraint::Unsigned { offset: 0, width, minimum: 0, maximum }],
        };
        policy.check_args(&vec![255; width as usize], &[]).unwrap();
        let mut bytes = policy.encode().unwrap();
        for offset in [14, 15, 81, 86, 88, 112, 128, instance::POLICY_BYTES - 1] {
            bytes[offset] = 1;
            assert!(InstancePolicy::decode(&bytes).is_err(), "reserved byte {offset}");
            bytes[offset] = 0;
        }
        for rule in [
            ArgsConstraint::Unsigned { offset: 0, width: 3, minimum: 0, maximum: 1 },
            ArgsConstraint::Unsigned { offset: 0, width: 1, minimum: 2, maximum: 1 },
            ArgsConstraint::Unsigned { offset: 0, width: 1, minimum: 0, maximum: 256 },
        ] {
            let mut bad = policy.clone();
            bad.constraints = vec![rule];
            assert_eq!(bad.encode(), Err(InstanceError::Bounds));
        }
    }
}

#[test]
fn wrapped_selection_retains_yank_floor_and_type_history_bytes() {
    let active = set(SelectionMode::Compatible, ScriptRole::Type, &[], MemberStatus::Active, 3);
    for (status, floor, expected) in [
        (MemberStatus::Yanked, 3, implementation::PolicyError::Inactive),
        (MemberStatus::Active, 6, implementation::PolicyError::Sequence),
    ] {
        let unavailable = set(SelectionMode::Compatible, ScriptRole::Type, &[], status, floor);
        let policy = make_policy(&unavailable, InstanceMode::Exact, &[]);
        assert_eq!(
            instance::encode_selection(&policy, &unavailable.selection(&[1; 32]).unwrap(), &[]),
            Err(InstanceError::Implementation(expected))
        );
    }
    let mut member = active.members()[0].clone();
    member.hash_type = CodeHashType::Type;
    member.code_hash = [20; 32];
    member.deployment_line = [20; 32];
    member.history_tip = [21; 32];
    member.deployment_sequence = 2;
    member.script = ckb_blake2b256(&instance::instance_script(&member.code_hash, CodeHashType::Type, &[]).unwrap());
    let set = AuthorizationSet::new(active.header().clone(), &[member.clone()]).unwrap();
    let policy = make_policy(&set, InstanceMode::Exact, &[]);
    let context = ConsumerArgs { policy_root: policy.root().unwrap(), subject: member.script, config: vec![] }.encode().unwrap();
    let selection = instance::encode_selection(&policy, &set.selection(&member.receipt).unwrap(), &[]).unwrap();
    let checked = instance::verify_selection(&context, &selection).unwrap();
    assert_eq!(checked.implementation().member(), &member);
    assert_eq!(checked.script_hash(), member.script);
    // Byte preservation is not a proof that the Type-ID history is valid.
}
