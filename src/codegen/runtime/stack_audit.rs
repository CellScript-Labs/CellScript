//! Generate the complete runtime/stdlib surface, not only helpers reached by
//! today's source fixtures, and inspect actual decoded machine instructions.
use super::*;

fn runtime_inventory(opt_level: u8) -> (Vec<String>, MachineLayoutEvidence) {
    // Discover literal helper names in the dispatch source so adding a helper
    // cannot silently leave it out of this all-helpers generation test.
    let mut names = include_str!("../runtime.rs")
        .split('"')
        .filter(|name| name.starts_with("__") && name.chars().all(|character| character.is_ascii_alphanumeric() || character == '_'))
        .map(str::to_string)
        .collect::<BTreeSet<_>>();
    names.extend(runtime_contract::SURFACE.iter().map(|entry| entry.0.to_string()));
    let ir = IrModule {
        name: "stack_inventory".into(),
        items: vec![],
        entry_selection: Default::default(),
        external_type_defs: vec![],
        external_callable_abis: vec![],
        enum_fixed_sizes: Default::default(),
        enum_layouts: Default::default(),
        generic_contracts: Default::default(),
        source_type_origins: Default::default(),
    };
    let mut generator = CodeGenerator::new(CodegenOptions { opt_level, ..Default::default() });
    generator.emit_section(".text");
    generator.emit_global("stack_inventory");
    generator.emit_label("stack_inventory");
    generator.emit("ret");
    generator.generate_runtime_support(&ir);
    generator.emit_runtime_cell_membership();
    generator.emit_runtime_ckb_v014_surface_helpers(&names);
    for (name, field) in [("__env_current_timepoint", 1), ("__ckb_header_epoch_number", 1)] {
        generator.emit_runtime_header_field_u64(name, "epoch", field, true, "");
    }
    for (name, field) in
        [("__ckb_header_dep_epoch_number", 1), ("__ckb_header_dep_epoch_start_block_number", 2), ("__ckb_header_dep_epoch_length", 3)]
    {
        generator.emit_runtime_header_dep_field_u64(name, "epoch", field, true, "");
    }
    for (name, offset) in [("__ckb_header_dep_block_number", 16), ("__ckb_header_dep_timestamp_millis", 8)] {
        generator.emit_runtime_header_dep_full_u64(name, "header", offset, true, "");
    }
    generator.emit_runtime_input_field_u64("__ckb_input_since", "since", CKB_INPUT_FIELD_SINCE, true, "");
    generator.emit_process_failure_helper();
    // Stdlib has an alternate __env_current_timepoint implementation. Rename
    // its complete internal namespace only in this inventory so both bodies
    // can be decoded together without silently dropping either implementation.
    generator.assembly.extend(crate::stdlib::StdLib::generate_assembly().replace("__", "__stdlib_").lines().map(str::to_string));
    generator
        .assembly
        .extend(crate::stdlib::collections::Collections::generate_assembly().replace("__", "__stdlib_").lines().map(str::to_string));
    assert!(generator.emission_error.is_none(), "{:?}", generator.emission_error);
    let evidence = assembler::machine_layout_evidence(&generator.assembly, &generator.entry_frame_sizes, &ir).unwrap();
    (generator.assembly, evidence)
}

#[test]
fn all_runtime_and_stdlib_machine_frames_are_aligned_and_restored() {
    for opt_level in [0, 3] {
        let (assembly, evidence) = runtime_inventory(opt_level);
        let artifact = assembler::assemble_generated_elf(&assembly).unwrap();
        let elf = cellscript_artifact_checker::parse_elf(&artifact, 1_000_000).unwrap();
        let mut problems = Vec::new();
        let mut owners = BTreeMap::new();
        for (name, address) in &evidence.symbols {
            if (evidence.globals.contains(name) || name.starts_with("__"))
                && (evidence.text_start..evidence.text_end).contains(address)
            {
                owners.insert(*address, name.as_str());
            }
        }
        for adjustment in &elf.stack_adjustments {
            if adjustment.delta % 16 != 0 {
                let owner = owners.range(..=adjustment.address).next_back().unwrap().1;
                problems.push(format!("{owner}: unaligned adjustment {} at {:#x}", adjustment.delta, adjustment.address));
            }
        }
        let by_start = evidence.blocks.iter().map(|block| (block.start, block.index)).collect::<BTreeMap<_, _>>();
        let mut incoming = BTreeMap::new();
        let mut pending = owners.keys().filter_map(|address| by_start.get(address).map(|index| (*index, 0i64))).collect::<Vec<_>>();
        let abort = evidence.symbols.get("__cellscript_abort").copied();
        while let Some((index, mut delta)) = pending.pop() {
            let block = &evidence.blocks[index];
            if abort == Some(block.start) {
                continue; // independently recognized nonreturning EXIT sink
            }
            if let Some(previous) = incoming.insert(index, delta) {
                if previous != delta {
                    problems.push(format!("{:?}: inconsistent incoming stack depths {previous}/{delta}", block.label));
                }
                continue;
            }
            for instruction in elf.instructions.iter().filter(|instruction| (block.start..block.end).contains(&instruction.address)) {
                if let Some(adjustment) = elf.stack_adjustments.iter().find(|site| site.address == instruction.address) {
                    delta += adjustment.delta;
                    if delta > 0 {
                        problems.push(format!("{:?}: stack restored beyond entry", block.label));
                    }
                }
                let word = instruction.word;
                let opcode = word & 0x7f;
                if matches!(opcode, 3 | 0x23) && word >> 15 & 31 == 2 {
                    let immediate = if opcode == 3 { word >> 20 } else { (word >> 7 & 31) | (word >> 25 << 5) };
                    let offset = i64::from((immediate as i32) << 20 >> 20);
                    let width = 1i64 << (word >> 12 & 3);
                    if offset < 0 || delta + offset + width > 0 {
                        problems.push(format!("{:?}: direct stack access {offset}+{width} exceeds frame {}", block.label, -delta));
                    }
                }
            }
            if block.terminator == MachineTerminatorEvidence::Return && delta != 0 {
                problems.push(format!("{:?}: returning with stack delta {delta}", block.label));
            }
            for edge in evidence.edges.iter().filter(|edge| edge.from == index && edge.kind != MachineEdgeKindEvidence::Call) {
                pending.push((edge.to, delta));
            }
        }
        for adjustment in &elf.stack_adjustments {
            if !evidence
                .blocks
                .iter()
                .any(|block| incoming.contains_key(&block.index) && (block.start..block.end).contains(&adjustment.address))
            {
                let owner = owners.range(..=adjustment.address).next_back().unwrap().1;
                let block = evidence.blocks.iter().find(|block| (block.start..block.end).contains(&adjustment.address)).unwrap();
                problems.push(format!(
                    "{owner}/{:?}: stack adjustment {} at {:#x} escaped the entry/CFG inventory",
                    block.label, adjustment.delta, adjustment.address
                ));
            }
        }
        eprintln!("O{opt_level}: {} runtime/stdlib entries, {} machine stack adjustments", owners.len(), elf.stack_adjustments.len());
        assert!(owners.len() > 150, "inventory unexpectedly lost helpers: {}", owners.len());
        assert!(problems.is_empty(), "O{opt_level} stack inventory:\n{}", problems.join("\n"));
    }
}

#[test]
fn compacted_instance_profiles_only_change_owned_frame_extents() {
    // Independently retain the previously admitted complete-body identities.
    // Restoring only the two SP immediates must reproduce each old identity;
    // updating a profile pin cannot silently bless another instruction change.
    let (assembly, evidence) = runtime_inventory(0);
    let artifact = assembler::assemble_generated_elf(&assembly).unwrap();
    let elf = cellscript_artifact_checker::parse_elf(&artifact, 1_000_000).unwrap();
    let owners = evidence
        .symbols
        .iter()
        .filter(|(name, address)| name.starts_with("__") && (evidence.text_start..evidence.text_end).contains(address))
        .map(|(name, address)| (*address, name.as_str()))
        .collect::<BTreeMap<_, _>>();
    for (name, old_frame, new_frame, old_hash) in [
        (
            "__ckb_require_instance_script_participant_from_args",
            1808i32,
            1792i32,
            "3ff70996e363e4e171ea23c25f278159fa69a4ef922c4521f4732d5c62a20983",
        ),
        ("__ckb_require_cell_dep_open_script_handle", 992, 976, "d21189ae305942f8153d8198cda764685c29ca352eb0af8ac73c01637c4d4ee4"),
    ] {
        let start = evidence.symbols[name];
        let end = owners.range(start + 1..).next().map_or(evidence.text_end, |(address, _)| *address);
        let instructions =
            elf.instructions.iter().filter(|instruction| (start..end).contains(&instruction.address)).collect::<Vec<_>>();
        let mut current = b"cellscript-instance-runtime-shape-v1\0".to_vec();
        current.extend_from_slice(name.as_bytes());
        current.push(0);
        let mut original = current.clone();
        let mut changes = 0;
        let mut index = 0;
        while index < instructions.len() {
            let instruction = instructions[index];
            let word = instruction.word;
            if word & 0x7f == 0x17 {
                let next = instructions[index + 1];
                assert_eq!(word & 0xfff, (1 << 7) | 0x17);
                assert_eq!(next.word & 0xfffff, (1 << 15) | (1 << 7) | 0x67);
                let displacement = i64::from((word & 0xfffff000) as i32) + i64::from((next.word as i32) >> 20);
                let target = instruction.address.checked_add_signed(displacement).unwrap() & !1;
                let callee = owners[&target];
                for bytes in [&mut original, &mut current] {
                    bytes.push(1);
                    bytes.extend_from_slice(&(callee.len() as u16).to_le_bytes());
                    bytes.extend_from_slice(callee.as_bytes());
                }
                index += 2;
                continue;
            }
            let mut previous = word;
            if word & 0xfffff == (2 << 15) | (2 << 7) | 0x13 {
                let delta = (word as i32) >> 20;
                assert_eq!(delta.abs(), new_frame);
                previous = (word & 0xfffff) | (((delta.signum() * old_frame) as u32 & 0xfff) << 20);
                changes += 1;
            }
            current.push(0);
            current.extend_from_slice(&word.to_le_bytes());
            original.push(0);
            original.extend_from_slice(&previous.to_le_bytes());
            index += 1;
        }
        assert_eq!(changes, 2, "{name}: allocation and restoration only");
        let hash = |bytes: &[u8]| cellscript_artifact_checker::hex_encode(&cellscript_artifact_checker::ckb_blake2b256(bytes));
        assert_eq!(hash(&original), old_hash, "{name}: another machine instruction changed");
        eprintln!("{name}={}", hash(&current));
    }
}

#[test]
fn all_runtime_entries_have_explicit_return_conventions() {
    let (_, evidence) = runtime_inventory(0);
    let missing = evidence
        .globals
        .iter()
        .filter(|name| name.starts_with("__") && !name.starts_with("__stdlib_") && runtime_contract::return_abi(name).is_none())
        .collect::<Vec<_>>();
    assert!(missing.is_empty(), "missing runtime return contracts: {missing:?}");
}

#[test]
fn scalar_status_contracts_write_status_on_every_returning_path() {
    use runtime_contract::RuntimeReturnAbi;
    let (assembly, evidence) = runtime_inventory(0);
    let artifact = assembler::assemble_generated_elf(&assembly).unwrap();
    let elf = cellscript_artifact_checker::parse_elf(&artifact, 1_000_000).unwrap();
    let words = elf.instructions.iter().map(|word| (word.address, word.word)).collect::<BTreeMap<_, _>>();
    let targets = elf.control_flow.iter().map(|edge| (edge.address, edge.target)).collect::<BTreeMap<_, _>>();
    let owners =
        evidence.globals.iter().filter_map(|name| evidence.symbols.get(name).map(|pc| (*pc, name))).collect::<BTreeMap<_, _>>();
    let abort = evidence.symbols["__cellscript_abort"];
    for (&start, name) in &owners {
        if !matches!(
            runtime_contract::return_abi(name),
            Some(RuntimeReturnAbi::ScalarStatusA1 | RuntimeReturnAbi::ScalarZeroStatusOrAbort)
        ) {
            continue;
        }
        let end = owners
            .range((std::ops::Bound::Excluded(start), std::ops::Bound::Unbounded))
            .next()
            .map_or(evidence.text_end, |(pc, _)| *pc);
        let mut pending = vec![(start, false)];
        let mut seen = BTreeSet::new();
        while let Some((pc, mut status_written)) = pending.pop() {
            if pc == abort || !seen.insert((pc, status_written)) {
                continue;
            }
            if !(start..end).contains(&pc) {
                let callee = owners.get(&pc).expect("tail target must be a complete inventoried entry");
                let abi = runtime_contract::return_abi(callee);
                assert!(
                    abi == Some(RuntimeReturnAbi::ScalarZeroStatusOrAbort)
                        || (runtime_contract::return_abi(name) == Some(RuntimeReturnAbi::ScalarStatusA1)
                            && abi == Some(RuntimeReturnAbi::ScalarStatusA1)),
                    "{name}: incompatible tail return convention at {callee}"
                );
                // The target's paths are checked independently by this same inventory.
                continue;
            }
            let word = words[&pc];
            if word == 0x00008067 {
                assert!(status_written, "{name}: returning without defining a1 at {pc:#x}");
                continue;
            }
            let opcode = word & 0x7f;
            if matches!(opcode, 0x6f | 0x67) {
                let target = targets[&pc];
                if word >> 7 & 31 == 1 {
                    let callee = owners.get(&target).expect("call target must have an inventoried owner");
                    if runtime_contract::return_abi(callee) == Some(RuntimeReturnAbi::NoReturn) {
                        continue;
                    }
                    status_written = matches!(
                        runtime_contract::return_abi(callee),
                        Some(RuntimeReturnAbi::ScalarStatusA1 | RuntimeReturnAbi::ScalarZeroStatusOrAbort)
                    );
                    pending.push((pc + 4, status_written));
                } else {
                    pending.push((target, status_written));
                }
                continue;
            }
            if opcode == 0x63 {
                pending.push((targets[&pc], status_written));
            }
            // A syscall's scratch argument a1 is not a returned error status.
            if word == 0x73 {
                status_written = false;
            }
            if matches!(opcode, 3 | 0x13 | 0x1b | 0x33 | 0x3b | 0x17 | 0x37) && word >> 7 & 31 == 11 {
                status_written = true;
            }
            pending.push((pc + 4, status_written));
        }
    }
}
