use super::{
    compile, compile_file, compile_file_with_entry_action, compile_file_with_entry_lock, compile_fungible_type_group_entry,
    compile_path, compile_path_metadata_with_diagnostics, compile_path_metadata_with_diagnostics_for_sources,
    compile_with_executable_surface_policy, decode_scheduler_witness_hex, default_output_path_for_input,
    encode_bounded_output_plan_v1, encode_entry_witness_args_for_params, incremental_cache_key, load_modules_for_input,
    prune_incremental_cache_entries, resolve_input_path, source_unit_from_bytes, validate_primitive_strict_017_metadata,
    ActionMetadata, ArtifactFormat, CellScriptEdition, CompileOptions, EntryWitnessArg, ExecutableSurfacePolicy,
    ENTRY_WITNESS_ABI_MAGIC, SCHEDULER_WITNESS_ABI_MOLECULE,
};
use crate::{ir, lexer, parser};
use camino::{Utf8Path, Utf8PathBuf};
use tempfile::tempdir;

include!("compile_pipeline/tests/lowering.rs");
include!("compile_pipeline/tests/lifecycle.rs");
include!("compile_pipeline/tests/values.rs");
include!("compile_pipeline/tests/metadata.rs");
include!("compile_pipeline/tests/ckb.rs");
include!("compile_pipeline/tests/packages.rs");

fn lock_package_for_test(root: &Utf8Path) -> crate::error::Result<()> {
    let mut manager = crate::package::PackageManager::new(root.as_std_path());
    let manifest = manager.read_manifest()?;
    let options = crate::package::active_resolution_options(crate::package::DependencyScope::Runtime);
    manager.resolve_dependencies_with_options(&options)?;

    let mut lockfile = crate::package::Lockfile::new();
    lockfile.package = crate::package::LockfilePackageInfo {
        edition: manifest.package.edition,
        name: manifest.package.name.clone(),
        version: manifest.package.version.clone(),
        namespace: manifest.package.namespace.clone(),
        source_hash: None,
        compiler_source_hash: None,
        compiler_requirement: manifest.package.cellscript_version.clone(),
        resolver_compiler_version: crate::VERSION.to_string(),
    };
    lockfile.replace_with_resolution(&manager, &manifest, &options)?;
    lockfile.write_to_root(root.as_std_path())
}

fn rebind_artifact_integrity_for_test(result: &mut crate::CompileResult) {
    result.artifact_hash = crate::ckb_blake2b256(&result.artifact_bytes);
    result.metadata.artifact_hash = Some(crate::hex_encode(&result.artifact_hash));
    result.metadata.artifact_size_bytes = Some(result.artifact_bytes.len());
}

fn parse_module_for_test(source: &str) -> crate::ast::Module {
    let tokens = lexer::lex(source).unwrap();
    parser::parse(&tokens).unwrap()
}

fn compile_metadata_for_profile_without_artifact_policy(source: &str, target_profile: crate::TargetProfile) -> crate::CompileMetadata {
    let ast = parse_module_for_test(source);
    crate::types::check(&ast).unwrap();
    crate::flow::check(&ast).unwrap();
    let ir = ir::generate(&ast).unwrap();
    let mut metadata =
        crate::compile_metadata_from_ir(&ir, ArtifactFormat::RiscvAssembly, target_profile, crate::CURRENT_EDITION, None);
    crate::bind_public_interface(&mut metadata, &ast, None).unwrap();
    crate::bind_typed_semantics(&mut metadata, &ir, &ast, None, false).unwrap();
    crate::validate_compile_metadata(&metadata, ArtifactFormat::RiscvAssembly).unwrap();
    metadata
}

const SIMPLE_PROGRAM: &str = r#"
module test;

action add(witness x: u64, witness y: u64) -> u64 {

        let z = x + y;
        return z;
}
"#;

#[derive(Debug)]
struct SchedulerAccessWitness {
    operation: u8,
    source: u8,
    index: u32,
    binding_hash: [u8; 32],
}

#[derive(Debug)]
struct SchedulerWitness {
    magic: u16,
    version: u8,
    effects: ir::EffectFacts,
    parallelizable: bool,
    touches_shared_count: u32,
    touches_shared: Vec<[u8; 32]>,
    estimated_cycles: u64,
    access_count: u32,
    accesses: Vec<SchedulerAccessWitness>,
}

fn decode_molecule_scheduler_witness_hex(hex: &str) -> SchedulerWitness {
    let bytes = decode_scheduler_witness_hex(hex).expect("scheduler witness hex should decode");
    let fields = decode_molecule_table(&bytes, 9);
    SchedulerWitness {
        magic: read_u16(fields[0], "magic"),
        version: read_u8(fields[1], "version"),
        effects: {
            let length = read_u32(&fields[2][..4], "effect bytes length") as usize;
            assert_eq!(length, fields[2].len() - 4);
            serde_json::from_slice(&fields[2][4..]).expect("current structured effect facts")
        },
        parallelizable: read_bool(fields[3], "parallelizable"),
        touches_shared_count: read_u32(fields[4], "touches_shared_count"),
        touches_shared: read_fixvec_byte32(fields[5]),
        estimated_cycles: read_u64(fields[6], "estimated_cycles"),
        access_count: read_u32(fields[7], "access_count"),
        accesses: read_scheduler_accesses(fields[8]),
    }
}

fn decode_molecule_table(bytes: &[u8], expected_fields: usize) -> Vec<&[u8]> {
    assert!(bytes.len() >= 8, "molecule table header is too short: {}", bytes.len());
    let total_size = read_u32(&bytes[..4], "total_size") as usize;
    assert_eq!(total_size, bytes.len(), "molecule table total size mismatch");
    let first_offset = read_u32(&bytes[4..8], "first_offset") as usize;
    assert!(first_offset >= 8 && first_offset <= bytes.len() && first_offset.is_multiple_of(4), "invalid first offset {first_offset}");
    let field_count = first_offset / 4 - 1;
    assert_eq!(field_count, expected_fields, "unexpected molecule table field count");
    let mut offsets = bytes[4..first_offset].chunks_exact(4).map(|chunk| read_u32(chunk, "offset") as usize).collect::<Vec<_>>();
    offsets.push(total_size);
    for pair in offsets.windows(2) {
        assert!(pair[0] <= pair[1], "molecule offsets must be monotonic: {:?}", offsets);
        assert!(pair[0] >= first_offset && pair[1] <= total_size, "molecule offsets must stay in payload: {:?}", offsets);
    }
    offsets.windows(2).map(|pair| &bytes[pair[0]..pair[1]]).collect()
}

fn read_scheduler_accesses(bytes: &[u8]) -> Vec<SchedulerAccessWitness> {
    let count = read_u32(&bytes[..4], "access_count") as usize;
    assert_eq!(bytes.len(), 4 + count * 38, "access fixvec byte length mismatch");
    bytes[4..]
        .chunks_exact(38)
        .map(|chunk| SchedulerAccessWitness {
            operation: chunk[0],
            source: chunk[1],
            index: read_u32(&chunk[2..6], "access.index"),
            binding_hash: chunk[6..38].try_into().expect("binding hash width"),
        })
        .collect()
}

fn read_fixvec_byte32(bytes: &[u8]) -> Vec<[u8; 32]> {
    let count = read_u32(&bytes[..4], "byte32_count") as usize;
    assert_eq!(bytes.len(), 4 + count * 32, "byte32 fixvec byte length mismatch");
    bytes[4..].chunks_exact(32).map(|chunk| chunk.try_into().expect("byte32 width")).collect()
}

fn read_u8(bytes: &[u8], field: &str) -> u8 {
    assert_eq!(bytes.len(), 1, "{field} should be a molecule byte");
    bytes[0]
}

fn read_bool(bytes: &[u8], field: &str) -> bool {
    match read_u8(bytes, field) {
        0 => false,
        1 => true,
        value => panic!("{field} should be a molecule bool, got {value}"),
    }
}

fn read_u16(bytes: &[u8], field: &str) -> u16 {
    assert_eq!(bytes.len(), 2, "{field} should be a molecule u16");
    u16::from_le_bytes(bytes.try_into().expect("u16 width"))
}

fn read_u16_at(bytes: &[u8], offset: usize, field: &str) -> u16 {
    read_u16(&bytes[offset..offset + 2], field)
}

fn read_u32(bytes: &[u8], field: &str) -> u32 {
    assert_eq!(bytes.len(), 4, "{field} should be a molecule u32");
    u32::from_le_bytes(bytes.try_into().expect("u32 width"))
}

fn read_u32_at(bytes: &[u8], offset: usize, field: &str) -> u32 {
    read_u32(&bytes[offset..offset + 4], field)
}

fn read_u64(bytes: &[u8], field: &str) -> u64 {
    assert_eq!(bytes.len(), 8, "{field} should be a molecule u64");
    u64::from_le_bytes(bytes.try_into().expect("u64 width"))
}

fn read_u64_at(bytes: &[u8], offset: usize, field: &str) -> u64 {
    read_u64(&bytes[offset..offset + 8], field)
}

fn executable_elf_entry_segment_for_test(elf: &[u8]) -> (u32, u64, u64, usize) {
    assert!(elf.starts_with(b"\x7fELF"), "expected ELF magic");
    let entry = read_u64_at(elf, 24, "e_entry");
    let phoff = usize::try_from(read_u64_at(elf, 32, "e_phoff")).expect("program header offset should fit usize");
    let phentsize = usize::from(read_u16_at(elf, 54, "e_phentsize"));
    let phnum = usize::from(read_u16_at(elf, 56, "e_phnum"));

    for index in 0..phnum {
        let offset = phoff + index * phentsize;
        let p_type = read_u32_at(elf, offset, "p_type");
        let flags = read_u32_at(elf, offset + 4, "p_flags");
        if p_type != 1 || flags & 1 == 0 {
            continue;
        }
        let p_offset = read_u64_at(elf, offset + 8, "p_offset");
        let p_vaddr = read_u64_at(elf, offset + 16, "p_vaddr");
        let file_size = read_u64_at(elf, offset + 32, "p_filesz");
        let memory_size = read_u64_at(elf, offset + 40, "p_memsz");
        let entry_file_offset = p_offset + entry.checked_sub(p_vaddr).expect("entrypoint should be inside executable segment");
        return (flags, file_size, memory_size, usize::try_from(entry_file_offset).expect("entry file offset should fit usize"));
    }

    panic!("expected an executable PT_LOAD segment");
}

include!("compile_pipeline/tests/core_programs.rs");
include!("compile_pipeline/tests/reference_programs.rs");
include!("compile_pipeline/tests/cell_operation_programs.rs");
include!("compile_pipeline/tests/call_and_return_programs.rs");
include!("compile_pipeline/tests/linear_branch_programs.rs");
include!("compile_pipeline/tests/array_programs.rs");
include!("compile_pipeline/tests/vector_programs.rs");
include!("compile_pipeline/tests/type_and_flow_programs.rs");
