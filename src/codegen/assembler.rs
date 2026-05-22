//! RISC-V assembler and ELF binary emitter for CellScript.
//!
//! Parses textual assembly produced by the code generator,
//! performs branch relaxation, builds a machine-level CFG,
//! and emits a minimal ELF binary suitable for CKB-VM execution.

use crate::error::{CompileError, Result};
use crate::runtime_errors::CellScriptRuntimeError;
use serde::Serialize;
use std::collections::{BTreeSet, HashMap};
use std::env;
use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

const CKB_SOURCE_INPUT: u64 = 0x01;
const CKB_SOURCE_OUTPUT: u64 = 0x02;
const CKB_SOURCE_CELL_DEP: u64 = 0x03;
const CKB_SOURCE_HEADER_DEP: u64 = 0x04;
const CKB_SOURCE_GROUP_FLAG: u64 = 0x0100_0000_0000_0000;
const CKB_SOURCE_GROUP_INPUT: u64 = CKB_SOURCE_GROUP_FLAG | CKB_SOURCE_INPUT;
const CKB_SOURCE_GROUP_OUTPUT: u64 = CKB_SOURCE_GROUP_FLAG | CKB_SOURCE_OUTPUT;
const CKB_SOURCE_GROUP_CELL_DEP: u64 = CKB_SOURCE_GROUP_FLAG | CKB_SOURCE_CELL_DEP;
const CKB_SOURCE_GROUP_HEADER_DEP: u64 = CKB_SOURCE_GROUP_FLAG | CKB_SOURCE_HEADER_DEP;
pub(crate) const ELF_HEADER_SIZE: usize = 64;
pub(crate) const ELF_PROGRAM_HEADER_SIZE: usize = 56;
pub(crate) const ELF_SEGMENT_ALIGN: usize = 0x1000;
pub(crate) const ELF_BASE_ADDR: u64 = 0x10000;
pub(crate) const CKB_SCRIPT_STACK_TOP: i64 = 0x3f0000;
pub(crate) const EXIT_SYSCALL_NUMBER: i64 = 93;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum SectionKind {
    Text,
    Rodata,
}

#[derive(Debug, Clone)]
pub(crate) enum AsmOp {
    Label(String),
    Instruction(Instruction),
    Word(u32),
    Byte(u8),
    Ascii(Vec<u8>),
    Align(usize),
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct SymbolDef {
    section: SectionKind,
    offset: usize,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct SectionLayout {
    text_base: u64,
    text_user_base: u64,
    rodata_base: u64,
}

impl SectionLayout {
    fn for_text_user_size(text_user_size: usize) -> Self {
        let trampoline_size = start_trampoline_size();
        let rodata_offset = align_up(trampoline_size + text_user_size, 8);
        Self {
            text_base: ELF_BASE_ADDR,
            text_user_base: ELF_BASE_ADDR + trampoline_size as u64,
            rodata_base: ELF_BASE_ADDR + rodata_offset as u64,
        }
    }

    fn rodata_offset(&self) -> Result<usize> {
        usize::try_from(self.rodata_base - self.text_base)
            .map_err(|_| CompileError::new("ELF rodata offset does not fit usize", crate::error::Span::default()))
    }
}

#[derive(Debug)]
pub(crate) struct MachineLayoutPlan {
    pub(crate) parsed: ParsedAssembly,
    pub(crate) layout: SectionLayout,
    pub(crate) cfg: MachineCfg,
    pub(crate) order: MachineLayoutOrder,
    pub(crate) metrics: BackendLayoutMetrics,
}

#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct BackendLayoutMetrics {
    pub(crate) text_size: usize,
    pub(crate) rodata_size: usize,
    pub(crate) executable_text_op_count: usize,
    pub(crate) covered_text_op_count: usize,
    pub(crate) relaxed_branch_count: usize,
    pub(crate) max_cond_branch_abs_distance: u64,
    pub(crate) machine_block_count: usize,
    pub(crate) max_machine_block_size: usize,
    pub(crate) conditional_branch_block_count: usize,
    pub(crate) labeled_machine_block_count: usize,
    pub(crate) machine_cfg_edge_count: usize,
    pub(crate) machine_call_edge_count: usize,
    pub(crate) unreachable_machine_block_count: usize,
    pub(crate) layout_order_block_count: usize,
    pub(crate) layout_order_text_size: usize,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub struct BackendShapeMetrics {
    pub text_size: usize,
    pub rodata_size: usize,
    pub executable_text_op_count: usize,
    pub covered_text_op_count: usize,
    pub relaxed_branch_count: usize,
    pub max_cond_branch_abs_distance: u64,
    pub machine_block_count: usize,
    pub max_machine_block_size: usize,
    pub conditional_branch_block_count: usize,
    pub labeled_machine_block_count: usize,
    pub machine_cfg_edge_count: usize,
    pub machine_call_edge_count: usize,
    pub unreachable_machine_block_count: usize,
    pub layout_order_block_count: usize,
    pub layout_order_text_size: usize,
}

impl From<BackendLayoutMetrics> for BackendShapeMetrics {
    fn from(metrics: BackendLayoutMetrics) -> Self {
        Self {
            text_size: metrics.text_size,
            rodata_size: metrics.rodata_size,
            executable_text_op_count: metrics.executable_text_op_count,
            covered_text_op_count: metrics.covered_text_op_count,
            relaxed_branch_count: metrics.relaxed_branch_count,
            max_cond_branch_abs_distance: metrics.max_cond_branch_abs_distance,
            machine_block_count: metrics.machine_block_count,
            max_machine_block_size: metrics.max_machine_block_size,
            conditional_branch_block_count: metrics.conditional_branch_block_count,
            labeled_machine_block_count: metrics.labeled_machine_block_count,
            machine_cfg_edge_count: metrics.machine_cfg_edge_count,
            machine_call_edge_count: metrics.machine_call_edge_count,
            unreachable_machine_block_count: metrics.unreachable_machine_block_count,
            layout_order_block_count: metrics.layout_order_block_count,
            layout_order_text_size: metrics.layout_order_text_size,
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) enum Instruction {
    Addi { rd: u8, rs1: u8, imm: i64 },
    Add { rd: u8, rs1: u8, rs2: u8 },
    Sub { rd: u8, rs1: u8, rs2: u8 },
    And { rd: u8, rs1: u8, rs2: u8 },
    Andi { rd: u8, rs1: u8, imm: i64 },
    Or { rd: u8, rs1: u8, rs2: u8 },
    Xor { rd: u8, rs1: u8, rs2: u8 },
    Mul { rd: u8, rs1: u8, rs2: u8 },
    Div { rd: u8, rs1: u8, rs2: u8 },
    Divu { rd: u8, rs1: u8, rs2: u8 },
    Rem { rd: u8, rs1: u8, rs2: u8 },
    Remu { rd: u8, rs1: u8, rs2: u8 },
    Slt { rd: u8, rs1: u8, rs2: u8 },
    Sltu { rd: u8, rs1: u8, rs2: u8 },
    Sgt { rd: u8, rs1: u8, rs2: u8 },
    Sgtu { rd: u8, rs1: u8, rs2: u8 },
    Xori { rd: u8, rs1: u8, imm: i64 },
    Seqz { rd: u8, rs: u8 },
    Snez { rd: u8, rs: u8 },
    Neg { rd: u8, rs: u8 },
    Ld { rd: u8, rs1: u8, imm: i64 },
    Lbu { rd: u8, rs1: u8, imm: i64 },
    Sb { rs2: u8, rs1: u8, imm: i64 },
    Sh { rs2: u8, rs1: u8, imm: i64 },
    Sw { rs2: u8, rs1: u8, imm: i64 },
    Sd { rs2: u8, rs1: u8, imm: i64 },
    Slli { rd: u8, rs1: u8, shamt: i64 },
    Srli { rd: u8, rs1: u8, shamt: i64 },
    Li { rd: u8, imm: i128 },
    La { rd: u8, label: String },
    Call { label: String },
    Jump { label: String },
    Beq { rs1: u8, rs2: u8, label: String },
    Bne { rs1: u8, rs2: u8, label: String },
    Blt { rs1: u8, rs2: u8, label: String },
    Bge { rs1: u8, rs2: u8, label: String },
    Bltu { rs1: u8, rs2: u8, label: String },
    Bgeu { rs1: u8, rs2: u8, label: String },
    Beqz { rs: u8, label: String },
    Bnez { rs: u8, label: String },
    Ret,
    Ecall,
}

pub(crate) fn assemble_elf(lines: &[String]) -> Result<Vec<u8>> {
    reject_unresolved_calls(lines)?;
    if let Some(external) = try_external_elf_toolchain(lines)? {
        return Ok(external);
    }
    assemble_elf_internal(lines)
}

fn reject_unresolved_calls(lines: &[String]) -> Result<()> {
    let mut labels = BTreeSet::new();
    let mut calls = BTreeSet::new();

    for line in lines {
        let Some(clean) = strip_comment(line) else {
            continue;
        };
        if let Some(label) = clean.strip_suffix(':') {
            labels.insert(label.trim().to_string());
            continue;
        }
        if let Some(target) = clean.strip_prefix("call ") {
            let target = target.trim();
            if !target.is_empty() {
                calls.insert(target.to_string());
            }
        }
    }

    let missing = calls.difference(&labels).cloned().collect::<Vec<_>>();
    if missing.is_empty() {
        return Ok(());
    }

    Err(CompileError::without_span(format!(
        "unresolved call target(s) in generated assembly: {}; production ELF emission requires all call targets to be lowered",
        missing.join(", ")
    )))
}

pub(crate) fn assemble_elf_internal(lines: &[String]) -> Result<Vec<u8>> {
    let plan = MachineLayoutPlan::build(lines)?;
    let parsed = &plan.parsed;
    let layout = plan.layout;
    let _layout_control_metrics = (
        plan.metrics.executable_text_op_count,
        plan.metrics.covered_text_op_count,
        plan.metrics.relaxed_branch_count,
        plan.metrics.max_cond_branch_abs_distance,
        plan.metrics.machine_block_count,
        plan.metrics.max_machine_block_size,
        plan.metrics.conditional_branch_block_count,
        plan.metrics.labeled_machine_block_count,
        plan.metrics.machine_cfg_edge_count,
        plan.metrics.machine_call_edge_count,
        plan.metrics.unreachable_machine_block_count,
        plan.metrics.layout_order_block_count,
        plan.metrics.layout_order_text_size,
        plan.cfg.blocks.len(),
        plan.cfg.edges.len(),
        plan.order.block_order.len(),
        plan.order.placed_blocks.len(),
        plan.order.text_size,
    );
    let entry_label = parsed.entry_label.as_deref().ok_or_else(|| {
        CompileError::new("ELF target requires at least one action or lock entry point", crate::error::Span::default())
    })?;
    let text_user_size = plan.metrics.text_size;
    let rodata_size = plan.metrics.rodata_size;
    let rodata_offset = layout.rodata_offset()?;
    let trampoline_size = start_trampoline_size();
    let mut text_bytes = Vec::with_capacity(trampoline_size + text_user_size);
    encode_li_sequence(&mut text_bytes, 2, i128::from(CKB_SCRIPT_STACK_TOP))?;
    if entry_requires_explicit_parameter_abi(lines, entry_label) {
        encode_li_sequence(&mut text_bytes, 10, 25)?;
    } else {
        let entry_addr = parsed.symbol_address(entry_label, &layout)?;
        let call_pc = layout.text_base + text_bytes.len() as u64;
        encode_call_sequence(&mut text_bytes, call_pc, entry_addr)?;
    }
    encode_li_sequence(&mut text_bytes, 17, i128::from(EXIT_SYSCALL_NUMBER))?;
    text_bytes.extend_from_slice(&encode_ecall().to_le_bytes());
    if text_bytes.len() != trampoline_size {
        return Err(CompileError::new(
            format!(
                "internal ELF trampoline size mismatch: emitted {} bytes, layout reserved {} bytes",
                text_bytes.len(),
                trampoline_size
            ),
            crate::error::Span::default(),
        ));
    }
    parsed.encode_section(SectionKind::Text, &mut text_bytes, &layout, trampoline_size)?;

    let mut rodata_bytes = Vec::with_capacity(rodata_size);
    parsed.encode_section(SectionKind::Rodata, &mut rodata_bytes, &layout, 0)?;

    let segment_file_payload_size = rodata_offset + rodata_bytes.len();
    let segment_file_offset = align_up(ELF_HEADER_SIZE + ELF_PROGRAM_HEADER_SIZE, ELF_SEGMENT_ALIGN);
    let load_segment_offset = 0u64;
    let load_segment_vaddr = layout.text_base.checked_sub(segment_file_offset as u64).ok_or_else(|| {
        CompileError::new("ELF text base is smaller than the load segment file offset", crate::error::Span::default())
    })?;
    let load_segment_file_size = segment_file_offset + segment_file_payload_size;
    let mut elf = vec![0u8; load_segment_file_size];
    write_elf_header(&mut elf[..ELF_HEADER_SIZE], layout.text_base, 1)?;
    write_program_header(
        &mut elf[ELF_HEADER_SIZE..ELF_HEADER_SIZE + ELF_PROGRAM_HEADER_SIZE],
        5,
        load_segment_offset,
        load_segment_vaddr,
        load_segment_file_size as u64,
        load_segment_file_size as u64,
    )?;

    let segment = &mut elf[segment_file_offset..segment_file_offset + segment_file_payload_size];
    segment[..text_bytes.len()].copy_from_slice(&text_bytes);
    segment[rodata_offset..rodata_offset + rodata_bytes.len()].copy_from_slice(&rodata_bytes);
    Ok(elf)
}

fn try_external_elf_toolchain(lines: &[String]) -> Result<Option<Vec<u8>>> {
    let Some(toolchain) = discover_external_toolchain()? else {
        return Ok(None);
    };
    let parsed = ParsedAssembly::from_lines(lines)?;
    let entry_label = parsed.entry_label.as_deref().ok_or_else(|| {
        CompileError::new("ELF target requires at least one action or lock entry point", crate::error::Span::default())
    })?;

    let temp_dir = make_external_toolchain_temp_dir()?;
    let _temp_dir_cleanup = TempDirCleanup(temp_dir.clone());
    let asm_path = temp_dir.join("module.s");
    let elf_path = temp_dir.join("module.elf");
    let obj_path = temp_dir.join("module.o");
    fs::write(&asm_path, render_external_assembly(lines, entry_label)).map_err(|err| {
        CompileError::new(
            format!("failed to write temporary assembly file '{}': {}", asm_path.display(), err),
            crate::error::Span::default(),
        )
    })?;

    let external_result = match &toolchain.mode {
        ExternalToolchainMode::Compiler(compiler) => run_external_command(
            Command::new(compiler)
                .arg("-nostdlib")
                .arg("-march=rv64imac")
                .arg("-mabi=lp64")
                .arg("-Wl,--strip-all")
                .arg("-Wl,-e,_start")
                .arg("-Wl,-Ttext=0x10000")
                .arg("-o")
                .arg(&elf_path)
                .arg(&asm_path),
            "RISC-V compiler",
        ),
        ExternalToolchainMode::AssemblerLinker { assembler, linker } => run_external_command(
            Command::new(assembler).arg("-march=rv64imac").arg("-mabi=lp64").arg(&asm_path).arg("-o").arg(&obj_path),
            "RISC-V assembler",
        )
        .and_then(|_| {
            run_external_command(
                Command::new(linker)
                    .arg("-m")
                    .arg("elf64lriscv")
                    .arg("--strip-all")
                    .arg("-e")
                    .arg("_start")
                    .arg("-Ttext")
                    .arg("0x10000")
                    .arg("-o")
                    .arg(&elf_path)
                    .arg(&obj_path),
                "RISC-V linker",
            )
        }),
    };

    let elf = match external_result {
        Ok(()) => fs::read(&elf_path).map_err(|err| {
            CompileError::new(
                format!("failed to read external ELF output '{}': {}", elf_path.display(), err),
                crate::error::Span::default(),
            )
        }),
        Err(err) => Err(err),
    };

    let elf = elf.and_then(|bytes| {
        if bytes.starts_with(b"\x7fELF") {
            Ok(bytes)
        } else {
            Err(CompileError::new(
                format!("external toolchain output '{}' is not an ELF file", elf_path.display()),
                crate::error::Span::default(),
            ))
        }
    })?;

    Ok(Some(elf))
}

struct TempDirCleanup(PathBuf);

impl Drop for TempDirCleanup {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn render_external_assembly(lines: &[String], entry_label: &str) -> String {
    let mut rendered =
        vec![".section .text".to_string(), ".global _start".to_string(), ".type _start, @function".to_string(), "_start:".to_string()];
    rendered.push(format!("    li sp, {}", CKB_SCRIPT_STACK_TOP));
    if entry_requires_explicit_parameter_abi(lines, entry_label) {
        let error = CellScriptRuntimeError::EntryWitnessAbiInvalid;
        rendered.push(format!("    # cellscript runtime error {} {}", error.code(), error.name()));
        rendered.push(format!("    li a0, {}", error.code()));
    } else {
        rendered.push(format!("    call {}", entry_label));
    }
    rendered.push(format!("    li a7, {}", EXIT_SYSCALL_NUMBER));
    rendered.push("    ecall".to_string());
    rendered.extend(lines.iter().filter(|line| !line.trim_start().starts_with(".option arch,")).cloned());
    let mut rendered = rendered.join("\n");
    rendered.push('\n');
    rendered
}

fn entry_requires_explicit_parameter_abi(lines: &[String], entry_label: &str) -> bool {
    let marker = format!("# cellscript entry abi: {} requires-explicit-parameter-abi", entry_label);
    lines.iter().any(|line| line.trim() == marker)
}

#[derive(Debug, Clone)]
struct ExternalToolchain {
    mode: ExternalToolchainMode,
}

#[derive(Debug, Clone)]
enum ExternalToolchainMode {
    Compiler(PathBuf),
    AssemblerLinker { assembler: PathBuf, linker: PathBuf },
}

fn discover_external_toolchain() -> Result<Option<ExternalToolchain>> {
    let explicit_compiler = explicit_toolchain_path("CELLSCRIPT_RISCV_CC")?;
    let explicit_assembler = explicit_toolchain_path("CELLSCRIPT_RISCV_AS")?;
    let explicit_linker = explicit_toolchain_path("CELLSCRIPT_RISCV_LD")?;

    if let Some(compiler) = explicit_compiler {
        if explicit_assembler.is_some() || explicit_linker.is_some() {
            return Err(CompileError::new(
                "set either CELLSCRIPT_RISCV_CC or CELLSCRIPT_RISCV_AS/CELLSCRIPT_RISCV_LD, not both",
                crate::error::Span::default(),
            ));
        }
        return Ok(Some(ExternalToolchain { mode: ExternalToolchainMode::Compiler(compiler) }));
    }

    match (explicit_assembler, explicit_linker) {
        (Some(assembler), Some(linker)) => {
            return Ok(Some(ExternalToolchain { mode: ExternalToolchainMode::AssemblerLinker { assembler, linker } }));
        }
        (Some(_), None) | (None, Some(_)) => {
            return Err(CompileError::new(
                "CELLSCRIPT_RISCV_AS and CELLSCRIPT_RISCV_LD must be set together",
                crate::error::Span::default(),
            ));
        }
        (None, None) => {}
    }

    Ok(None)
}

fn explicit_toolchain_path(var: &str) -> Result<Option<PathBuf>> {
    env::var_os(var).map(PathBuf::from).map(|path| validate_explicit_toolchain_path(var, path)).transpose()
}

pub(crate) fn validate_explicit_toolchain_path(var: &str, path: PathBuf) -> Result<PathBuf> {
    if !path.is_absolute() {
        return Err(CompileError::new(
            format!("{} must be an absolute path, got '{}'", var, path.display()),
            crate::error::Span::default(),
        ));
    }

    let metadata = fs::metadata(&path).map_err(|err| {
        CompileError::new(
            format!("{} points to unreadable toolchain path '{}': {}", var, path.display(), err),
            crate::error::Span::default(),
        )
    })?;
    if !metadata.is_file() {
        return Err(CompileError::new(
            format!("{} must point to an executable file, got '{}'", var, path.display()),
            crate::error::Span::default(),
        ));
    }

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        if metadata.permissions().mode() & 0o111 == 0 {
            return Err(CompileError::new(
                format!("{} path '{}' is not executable", var, path.display()),
                crate::error::Span::default(),
            ));
        }
    }

    Ok(path)
}

fn run_external_command(command: &mut Command, label: &str) -> Result<()> {
    let rendered = render_command(command);
    let output = command.output().map_err(|err| {
        CompileError::new(format!("failed to launch {} ({}): {}", label, rendered, err), crate::error::Span::default())
    })?;
    if output.status.success() {
        return Ok(());
    }

    let stderr = String::from_utf8_lossy(&output.stderr);
    let message = format!("{} failed ({}): {}", label, rendered, stderr.trim());
    Err(CompileError::new(message, crate::error::Span::default()))
}

fn render_command(command: &Command) -> String {
    let program = command.get_program().to_string_lossy();
    let args = command.get_args().map(|arg| arg.to_string_lossy().into_owned()).collect::<Vec<_>>().join(" ");
    if args.is_empty() {
        program.into_owned()
    } else {
        format!("{} {}", program, args)
    }
}

fn make_external_toolchain_temp_dir() -> Result<PathBuf> {
    let stamp = SystemTime::now().duration_since(UNIX_EPOCH).map(|duration| duration.as_nanos()).unwrap_or_default();
    let dir = env::temp_dir().join(format!("cellscript-riscv-{}-{}", std::process::id(), stamp));
    fs::create_dir_all(&dir).map_err(|err| {
        CompileError::new(
            format!("failed to create temporary toolchain directory '{}': {}", dir.display(), err),
            crate::error::Span::default(),
        )
    })?;
    Ok(dir)
}

#[derive(Debug, Default)]
pub(crate) struct ParsedAssembly {
    pub(crate) text_ops: Vec<AsmOp>,
    pub(crate) rodata_ops: Vec<AsmOp>,
    pub(crate) text_size: usize,
    pub(crate) rodata_size: usize,
    pub(crate) symbols: HashMap<String, SymbolDef>,
    pub(crate) entry_label: Option<String>,
    pub(crate) relaxed_text_branches: BTreeSet<usize>,
}

impl ParsedAssembly {
    pub(crate) fn from_lines(lines: &[String]) -> Result<Self> {
        Self::from_lines_with_branch_mode(lines, BranchSizeMode::Exact(&BTreeSet::new()))
    }

    pub(crate) fn from_lines_relaxed(lines: &[String], layout: &SectionLayout) -> Result<Self> {
        let conservative = Self::from_lines_with_branch_mode(lines, BranchSizeMode::Conservative)?;
        let relaxed_text_branches = conservative.relaxed_branch_indices(layout)?;
        Self::from_lines_with_branch_mode(lines, BranchSizeMode::Exact(&relaxed_text_branches))
    }

    pub(crate) fn from_lines_with_branch_mode(lines: &[String], branch_size_mode: BranchSizeMode<'_>) -> Result<Self> {
        let mut current_section = SectionKind::Text;
        let mut text_size = 0usize;
        let mut rodata_size = 0usize;
        let mut text_ops = Vec::new();
        let mut rodata_ops = Vec::new();
        let mut symbols = HashMap::new();
        let mut globals = BTreeSet::new();
        let mut entry_label = None;
        let mut fallback_entry = None;

        for (line_index, line) in lines.iter().enumerate() {
            let Some(clean) = strip_comment(line) else {
                continue;
            };
            if clean.is_empty() {
                continue;
            }

            if let Some(section) = parse_section_directive(clean).map_err(|err| assembly_line_error(line_index + 1, clean, err))? {
                current_section = section;
                continue;
            }
            if clean.starts_with(".option ") || clean.starts_with(".type ") {
                continue;
            }
            if let Some(symbol) = clean.strip_prefix(".global ") {
                globals.insert(symbol.trim().to_string());
                continue;
            }

            let (ops, offset) = match current_section {
                SectionKind::Text => (&mut text_ops, &mut text_size),
                SectionKind::Rodata => (&mut rodata_ops, &mut rodata_size),
            };
            let op_index = ops.len();

            if let Some(label) = clean.strip_suffix(':') {
                let label = label.trim().to_string();
                let symbol = SymbolDef { section: current_section, offset: *offset };
                if symbols.insert(label.clone(), symbol).is_some() {
                    return Err(CompileError::new(
                        format!("assembly line {} duplicate label '{}': {}", line_index + 1, label, clean),
                        crate::error::Span::default(),
                    ));
                }
                if current_section == SectionKind::Text && globals.contains(&label) {
                    if fallback_entry.is_none() {
                        fallback_entry = Some(label.clone());
                    }
                    if !label.starts_with("__") && entry_label.is_none() {
                        entry_label = Some(label.clone());
                    }
                }
                ops.push(AsmOp::Label(label));
                continue;
            }

            let op = parse_asm_op(clean).map_err(|err| assembly_line_error(line_index + 1, clean, err))?;
            *offset += op_size(&op, *offset, current_section, op_index, branch_size_mode);
            ops.push(op);
        }

        Ok(Self {
            text_ops,
            rodata_ops,
            text_size,
            rodata_size,
            symbols,
            entry_label: entry_label.or(fallback_entry),
            relaxed_text_branches: branch_size_mode.relaxed_text_branches().cloned().unwrap_or_default(),
        })
    }

    pub(crate) fn relaxed_branch_indices(&self, layout: &SectionLayout) -> Result<BTreeSet<usize>> {
        let mut relaxed = BTreeSet::new();
        let mut offset = 0usize;
        for (index, op) in self.text_ops.iter().enumerate() {
            if let AsmOp::Instruction(inst) = op {
                if conditional_branch_parts(inst).is_some() {
                    let pc = layout.text_user_base + offset as u64;
                    let target = branch_target(inst, self, layout)?;
                    if !signed_bits_fit(relative_offset(pc, target)?, 13) {
                        relaxed.insert(index);
                    }
                } else if let Instruction::Jump { label } = inst {
                    let pc = layout.text_user_base + offset as u64;
                    let target = self.symbol_address(label, layout)?;
                    if !signed_bits_fit(relative_offset(pc, target)?, 21) {
                        relaxed.insert(index);
                    }
                }
            }
            offset += op_size(op, offset, SectionKind::Text, index, BranchSizeMode::Conservative);
        }
        Ok(relaxed)
    }

    pub(crate) fn section_size(&self, section: SectionKind) -> usize {
        match section {
            SectionKind::Text => self.text_size,
            SectionKind::Rodata => self.rodata_size,
        }
    }

    pub(crate) fn symbol_address(&self, label: &str, layout: &SectionLayout) -> Result<u64> {
        let symbol = self
            .symbols
            .get(label)
            .ok_or_else(|| CompileError::new(format!("unknown assembly label '{}'", label), crate::error::Span::default()))?;
        Ok(match symbol.section {
            SectionKind::Text => layout.text_user_base + symbol.offset as u64,
            SectionKind::Rodata => layout.rodata_base + symbol.offset as u64,
        })
    }

    pub(crate) fn encode_section(
        &self,
        section: SectionKind,
        out: &mut Vec<u8>,
        layout: &SectionLayout,
        base_bias: usize,
    ) -> Result<()> {
        let ops = match section {
            SectionKind::Text => &self.text_ops,
            SectionKind::Rodata => &self.rodata_ops,
        };
        let section_base = match section {
            SectionKind::Text => layout.text_user_base,
            SectionKind::Rodata => layout.rodata_base,
        };

        for (op_index, op) in ops.iter().enumerate() {
            match op {
                AsmOp::Label(_) => {}
                AsmOp::Word(word) => out.extend_from_slice(&word.to_le_bytes()),
                AsmOp::Byte(byte) => out.push(*byte),
                AsmOp::Ascii(bytes) => out.extend_from_slice(bytes),
                AsmOp::Align(bytes) => pad_to_alignment(out, *bytes),
                AsmOp::Instruction(inst) => {
                    let section_offset = out.len().checked_sub(base_bias).ok_or_else(|| {
                        CompileError::new("assembly output offset is smaller than section base bias", crate::error::Span::default())
                    })?;
                    let pc = section_base + section_offset as u64;
                    encode_instruction(
                        out,
                        inst,
                        pc,
                        self,
                        layout,
                        section == SectionKind::Text && self.relaxed_text_branches.contains(&op_index),
                    )?;
                }
            }
        }

        Ok(())
    }
}

impl MachineLayoutPlan {
    pub(crate) fn build(lines: &[String]) -> Result<Self> {
        let preliminary = ParsedAssembly::from_lines_with_branch_mode(lines, BranchSizeMode::Conservative)?;
        let preliminary_layout = SectionLayout::for_text_user_size(preliminary.section_size(SectionKind::Text));
        let parsed = ParsedAssembly::from_lines_relaxed(lines, &preliminary_layout)?;
        let layout = SectionLayout::for_text_user_size(parsed.section_size(SectionKind::Text));
        let cfg = machine_cfg(&parsed)?;
        let coverage = validate_machine_block_coverage(&parsed, &cfg)?;
        let order = machine_layout_order(&cfg)?;
        let metrics = parsed.layout_metrics(&layout, &cfg, &order, coverage)?;
        Ok(Self { parsed, layout, cfg, order, metrics })
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct TextOpLayout {
    op_index: usize,
    offset: usize,
    size: usize,
}

#[derive(Debug, Clone)]
pub(crate) struct MachineBlock {
    pub(crate) label: Option<String>,
    pub(crate) op_start: usize,
    pub(crate) op_end: usize,
    pub(crate) byte_start: usize,
    pub(crate) byte_size: usize,
    pub(crate) terminator: MachineTerminator,
}

#[derive(Debug, Clone)]
pub(crate) struct MachineCfg {
    pub(crate) blocks: Vec<MachineBlock>,
    pub(crate) edges: Vec<MachineCfgEdge>,
}

#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct MachineBlockCoverage {
    executable_text_op_count: usize,
    covered_text_op_count: usize,
}

#[derive(Debug, Clone)]
pub(crate) struct MachineLayoutOrder {
    pub(crate) block_order: Vec<usize>,
    pub(crate) placed_blocks: Vec<MachinePlacedBlock>,
    pub(crate) text_size: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct MachinePlacedBlock {
    pub(crate) block_index: usize,
    pub(crate) byte_start: usize,
    pub(crate) byte_size: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct MachineCfgEdge {
    pub(crate) from: usize,
    pub(crate) to: usize,
    pub(crate) kind: MachineCfgEdgeKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MachineCfgEdgeKind {
    Fallthrough,
    Jump,
    ConditionalTaken,
    ConditionalFallthrough,
    Call,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum MachineTerminator {
    Fallthrough,
    Jump { target: String },
    ConditionalBranch { target: String },
    Return,
}

fn text_op_layouts(parsed: &ParsedAssembly) -> Vec<TextOpLayout> {
    let mut offset = 0usize;
    let mut layouts = Vec::with_capacity(parsed.text_ops.len());
    for (op_index, op) in parsed.text_ops.iter().enumerate() {
        let size = op_size(op, offset, SectionKind::Text, op_index, BranchSizeMode::Exact(&parsed.relaxed_text_branches));
        layouts.push(TextOpLayout { op_index, offset, size });
        offset += size;
    }
    layouts
}

fn machine_blocks(parsed: &ParsedAssembly) -> Vec<MachineBlock> {
    let layouts = text_op_layouts(parsed);
    let mut blocks = Vec::new();
    let mut block_start = 0usize;
    let mut block_label = None;

    for (op_index, op) in parsed.text_ops.iter().enumerate() {
        if let AsmOp::Label(label) = op {
            if block_has_executable_ops(&parsed.text_ops[block_start..op_index]) {
                blocks.push(build_machine_block(parsed, &layouts, block_start, op_index, block_label.take()));
                block_start = op_index;
            }
            if block_label.is_none() {
                block_label = Some(label.clone());
            }
            continue;
        }

        if instruction_terminator(op).is_some() {
            blocks.push(build_machine_block(parsed, &layouts, block_start, op_index + 1, block_label.take()));
            block_start = op_index + 1;
        }
    }

    if block_start < parsed.text_ops.len() && block_has_executable_ops(&parsed.text_ops[block_start..]) {
        blocks.push(build_machine_block(parsed, &layouts, block_start, parsed.text_ops.len(), block_label));
    }

    blocks
}

fn machine_cfg(parsed: &ParsedAssembly) -> Result<MachineCfg> {
    let blocks = machine_blocks(parsed);
    let label_to_block = machine_label_to_block(parsed, &blocks);
    let mut edges = Vec::new();

    for (index, block) in blocks.iter().enumerate() {
        for target in machine_block_call_targets(parsed, block) {
            if let Some(&target_block) = label_to_block.get(&target) {
                edges.push(MachineCfgEdge { from: index, to: target_block, kind: MachineCfgEdgeKind::Call });
            }
        }
        match &block.terminator {
            MachineTerminator::Fallthrough => {
                if index + 1 < blocks.len() {
                    edges.push(MachineCfgEdge { from: index, to: index + 1, kind: MachineCfgEdgeKind::Fallthrough });
                }
            }
            MachineTerminator::Jump { target } => {
                edges.push(MachineCfgEdge {
                    from: index,
                    to: machine_cfg_target_block(target, &label_to_block)?,
                    kind: MachineCfgEdgeKind::Jump,
                });
            }
            MachineTerminator::ConditionalBranch { target } => {
                edges.push(MachineCfgEdge {
                    from: index,
                    to: machine_cfg_target_block(target, &label_to_block)?,
                    kind: MachineCfgEdgeKind::ConditionalTaken,
                });
                if index + 1 < blocks.len() {
                    edges.push(MachineCfgEdge { from: index, to: index + 1, kind: MachineCfgEdgeKind::ConditionalFallthrough });
                }
            }
            MachineTerminator::Return => {}
        }
    }

    Ok(MachineCfg { blocks, edges })
}

fn validate_machine_block_coverage(parsed: &ParsedAssembly, cfg: &MachineCfg) -> Result<MachineBlockCoverage> {
    let executable_text_op_count = parsed.text_ops.iter().filter(|op| !matches!(op, AsmOp::Label(_))).count();
    let mut covered = BTreeSet::new();

    for block in &cfg.blocks {
        if block.op_start >= block.op_end || block.op_end > parsed.text_ops.len() {
            return Err(CompileError::new(
                format!("machine block has invalid op range {}..{}", block.op_start, block.op_end),
                crate::error::Span::default(),
            ));
        }
        if !block_has_executable_ops(&parsed.text_ops[block.op_start..block.op_end]) {
            return Err(CompileError::new("machine block contains no executable instructions", crate::error::Span::default()));
        }
        for op_index in block.op_start..block.op_end {
            if matches!(parsed.text_ops[op_index], AsmOp::Label(_)) {
                continue;
            }
            if !covered.insert(op_index) {
                return Err(CompileError::new(
                    format!("machine block coverage overlaps text op {}", op_index),
                    crate::error::Span::default(),
                ));
            }
        }
    }

    if covered.len() != executable_text_op_count {
        return Err(CompileError::new(
            format!("machine blocks cover {} executable text ops but assembly contains {}", covered.len(), executable_text_op_count),
            crate::error::Span::default(),
        ));
    }

    Ok(MachineBlockCoverage { executable_text_op_count, covered_text_op_count: covered.len() })
}

fn machine_layout_order(cfg: &MachineCfg) -> Result<MachineLayoutOrder> {
    let block_order = (0..cfg.blocks.len()).collect::<Vec<_>>();
    build_machine_layout_order(cfg, block_order)
}

pub(crate) fn build_machine_layout_order(cfg: &MachineCfg, block_order: Vec<usize>) -> Result<MachineLayoutOrder> {
    validate_machine_layout_order(cfg, &block_order)?;
    let mut byte_start = 0usize;
    let mut placed_blocks = Vec::with_capacity(block_order.len());
    for &block_index in &block_order {
        let block = &cfg.blocks[block_index];
        placed_blocks.push(MachinePlacedBlock { block_index, byte_start, byte_size: block.byte_size });
        byte_start += block.byte_size;
    }
    Ok(MachineLayoutOrder { block_order, placed_blocks, text_size: byte_start })
}

pub(crate) fn validate_machine_layout_order(cfg: &MachineCfg, block_order: &[usize]) -> Result<()> {
    if block_order.len() != cfg.blocks.len() {
        return Err(CompileError::new(
            format!("machine layout order contains {} blocks but CFG contains {}", block_order.len(), cfg.blocks.len()),
            crate::error::Span::default(),
        ));
    }

    let mut seen = BTreeSet::new();
    for &block_index in block_order {
        if block_index >= cfg.blocks.len() {
            return Err(CompileError::new(
                format!("machine layout order references missing block {}", block_index),
                crate::error::Span::default(),
            ));
        }
        if !seen.insert(block_index) {
            return Err(CompileError::new(
                format!("machine layout order repeats block {}", block_index),
                crate::error::Span::default(),
            ));
        }
    }

    Ok(())
}

fn machine_label_to_block(parsed: &ParsedAssembly, blocks: &[MachineBlock]) -> HashMap<String, usize> {
    let mut label_to_block = HashMap::new();
    for (label, symbol) in &parsed.symbols {
        if symbol.section != SectionKind::Text {
            continue;
        }
        if let Some((block_index, _)) = blocks.iter().enumerate().find(|(_, block)| block.byte_start == symbol.offset) {
            label_to_block.insert(label.clone(), block_index);
        }
    }
    label_to_block
}

fn machine_cfg_target_block(target: &str, label_to_block: &HashMap<String, usize>) -> Result<usize> {
    label_to_block.get(target).copied().ok_or_else(|| {
        CompileError::new(format!("assembly branch target '{}' does not start a machine block", target), crate::error::Span::default())
    })
}

fn machine_block_call_targets(parsed: &ParsedAssembly, block: &MachineBlock) -> Vec<String> {
    parsed.text_ops[block.op_start..block.op_end]
        .iter()
        .filter_map(|op| match op {
            AsmOp::Instruction(Instruction::Call { label }) => Some(label.clone()),
            _ => None,
        })
        .collect()
}

pub(crate) fn unreachable_machine_block_count(parsed: &ParsedAssembly, cfg: &MachineCfg) -> usize {
    if cfg.blocks.is_empty() {
        return 0;
    }
    let label_to_block = machine_label_to_block(parsed, &cfg.blocks);
    let mut roots = parsed.entry_label.as_ref().and_then(|label| label_to_block.get(label).copied()).into_iter().collect::<Vec<_>>();
    if roots.is_empty() {
        roots.push(0);
    }
    let mut reachable = BTreeSet::new();
    let mut stack = roots;
    while let Some(block) = stack.pop() {
        if !reachable.insert(block) {
            continue;
        }
        for edge in cfg.edges.iter().filter(|edge| edge.from == block) {
            stack.push(edge.to);
        }
    }
    cfg.blocks.len().saturating_sub(reachable.len())
}

fn block_has_executable_ops(ops: &[AsmOp]) -> bool {
    ops.iter().any(|op| !matches!(op, AsmOp::Label(_)))
}

fn build_machine_block(
    parsed: &ParsedAssembly,
    layouts: &[TextOpLayout],
    op_start: usize,
    op_end: usize,
    label: Option<String>,
) -> MachineBlock {
    let byte_start = layouts.get(op_start).map(|layout| layout.offset).unwrap_or(0);
    let byte_end =
        op_end.checked_sub(1).and_then(|last| layouts.get(last).map(|layout| layout.offset + layout.size)).unwrap_or(byte_start);
    let terminator =
        parsed.text_ops[op_start..op_end].iter().rev().find_map(instruction_terminator).unwrap_or(MachineTerminator::Fallthrough);
    MachineBlock { label, op_start, op_end, byte_start, byte_size: byte_end.saturating_sub(byte_start), terminator }
}

fn instruction_terminator(op: &AsmOp) -> Option<MachineTerminator> {
    match op {
        AsmOp::Instruction(Instruction::Jump { label }) => Some(MachineTerminator::Jump { target: label.clone() }),
        AsmOp::Instruction(Instruction::Ret) => Some(MachineTerminator::Return),
        AsmOp::Instruction(inst) => {
            conditional_branch_parts(inst).map(|(_, _, label, _)| MachineTerminator::ConditionalBranch { target: label.to_string() })
        }
        _ => None,
    }
}

impl ParsedAssembly {
    pub(crate) fn layout_metrics(
        &self,
        layout: &SectionLayout,
        machine_cfg: &MachineCfg,
        machine_order: &MachineLayoutOrder,
        coverage: MachineBlockCoverage,
    ) -> Result<BackendLayoutMetrics> {
        let text_op_layouts = text_op_layouts(self);
        let text_size = text_op_layouts.iter().map(|op| op.size).sum();
        let mut max_cond_branch_abs_distance = 0u64;
        for op_layout in text_op_layouts {
            let AsmOp::Instruction(inst) = &self.text_ops[op_layout.op_index] else {
                continue;
            };
            if conditional_branch_parts(inst).is_none() {
                continue;
            };
            let pc = layout.text_user_base + op_layout.offset as u64;
            let target = branch_target(inst, self, layout)?;
            let distance = relative_offset(pc, target)?.unsigned_abs();
            max_cond_branch_abs_distance = max_cond_branch_abs_distance.max(distance);
        }
        let machine_block_count = machine_cfg.blocks.len();
        let max_machine_block_size = machine_cfg.blocks.iter().map(|block| block.byte_size).max().unwrap_or_default();
        let conditional_branch_block_count =
            machine_cfg.blocks.iter().filter(|block| matches!(block.terminator, MachineTerminator::ConditionalBranch { .. })).count();
        let labeled_machine_block_count = machine_cfg.blocks.iter().filter(|block| block.label.is_some()).count();
        let machine_cfg_edge_count = machine_cfg.edges.len();
        let machine_call_edge_count = machine_cfg.edges.iter().filter(|edge| edge.kind == MachineCfgEdgeKind::Call).count();
        let unreachable_machine_block_count = unreachable_machine_block_count(self, machine_cfg);
        let layout_order_block_count = machine_order.block_order.len();
        let layout_order_text_size = machine_order.text_size;
        let _covered_text_ops = machine_cfg.blocks.iter().map(|block| block.op_end.saturating_sub(block.op_start)).sum::<usize>();
        let _first_block_byte_start = machine_cfg.blocks.first().map(|block| block.byte_start).unwrap_or_default();
        Ok(BackendLayoutMetrics {
            text_size,
            rodata_size: self.section_size(SectionKind::Rodata),
            executable_text_op_count: coverage.executable_text_op_count,
            covered_text_op_count: coverage.covered_text_op_count,
            relaxed_branch_count: self.relaxed_text_branches.len(),
            max_cond_branch_abs_distance,
            machine_block_count,
            max_machine_block_size,
            conditional_branch_block_count,
            labeled_machine_block_count,
            machine_cfg_edge_count,
            machine_call_edge_count,
            unreachable_machine_block_count,
            layout_order_block_count,
            layout_order_text_size,
        })
    }
}

fn parse_section_directive(line: &str) -> Result<Option<SectionKind>> {
    if let Some(section) = line.strip_prefix(".section ") {
        return match section.trim() {
            ".text" => Ok(Some(SectionKind::Text)),
            ".rodata" => Ok(Some(SectionKind::Rodata)),
            other => Err(CompileError::new(format!("unsupported assembly section '{}'", other), crate::error::Span::default())),
        };
    }
    Ok(None)
}

fn assembly_line_error(line_number: usize, line: &str, error: CompileError) -> CompileError {
    CompileError::new(format!("assembly line {} '{}': {}", line_number, line, error.message), error.span)
}

fn parse_asm_op(line: &str) -> Result<AsmOp> {
    if let Some(value) = line.strip_prefix(".word ") {
        let value = parse_immediate(value.trim())?;
        return Ok(AsmOp::Word(
            u32::try_from(value).map_err(|_| {
                CompileError::new(format!("'.word' value '{}' does not fit u32", value), crate::error::Span::default())
            })?,
        ));
    }
    if let Some(value) = line.strip_prefix(".byte ") {
        let value = parse_immediate(value.trim())?;
        return Ok(AsmOp::Byte(
            u8::try_from(value)
                .map_err(|_| CompileError::new(format!("'.byte' value '{}' does not fit u8", value), crate::error::Span::default()))?,
        ));
    }
    if let Some(value) = line.strip_prefix(".ascii ") {
        return Ok(AsmOp::Ascii(parse_ascii_literal(value.trim())?));
    }
    if let Some(value) = line.strip_prefix(".align ") {
        let align_pow = parse_immediate(value.trim())?;
        if !(0..=16).contains(&align_pow) {
            return Err(CompileError::new(format!("unsupported .align value '{}'", align_pow), crate::error::Span::default()));
        }
        return Ok(AsmOp::Align(1usize << (align_pow as usize)));
    }
    Ok(AsmOp::Instruction(parse_instruction(line)?))
}

fn parse_instruction(line: &str) -> Result<Instruction> {
    let mut parts = line.splitn(2, char::is_whitespace);
    let opcode =
        parts.next().ok_or_else(|| CompileError::new("malformed assembly instruction", crate::error::Span::default()))?.trim();
    let args = parts.next().unwrap_or("").trim();
    let args = if args.is_empty() { Vec::new() } else { args.split(',').map(|arg| arg.trim().to_string()).collect() };

    match opcode {
        "addi" => Ok(Instruction::Addi {
            rd: parse_register(arg(&args, 0)?)?,
            rs1: parse_register(arg(&args, 1)?)?,
            imm: parse_immediate(arg(&args, 2)?)?,
        }),
        "add" => Ok(Instruction::Add {
            rd: parse_register(arg(&args, 0)?)?,
            rs1: parse_register(arg(&args, 1)?)?,
            rs2: parse_register(arg(&args, 2)?)?,
        }),
        "sub" => Ok(Instruction::Sub {
            rd: parse_register(arg(&args, 0)?)?,
            rs1: parse_register(arg(&args, 1)?)?,
            rs2: parse_register(arg(&args, 2)?)?,
        }),
        "and" => Ok(Instruction::And {
            rd: parse_register(arg(&args, 0)?)?,
            rs1: parse_register(arg(&args, 1)?)?,
            rs2: parse_register(arg(&args, 2)?)?,
        }),
        "andi" => Ok(Instruction::Andi {
            rd: parse_register(arg(&args, 0)?)?,
            rs1: parse_register(arg(&args, 1)?)?,
            imm: parse_immediate(arg(&args, 2)?)?,
        }),
        "or" => Ok(Instruction::Or {
            rd: parse_register(arg(&args, 0)?)?,
            rs1: parse_register(arg(&args, 1)?)?,
            rs2: parse_register(arg(&args, 2)?)?,
        }),
        "xor" => Ok(Instruction::Xor {
            rd: parse_register(arg(&args, 0)?)?,
            rs1: parse_register(arg(&args, 1)?)?,
            rs2: parse_register(arg(&args, 2)?)?,
        }),
        "mul" => Ok(Instruction::Mul {
            rd: parse_register(arg(&args, 0)?)?,
            rs1: parse_register(arg(&args, 1)?)?,
            rs2: parse_register(arg(&args, 2)?)?,
        }),
        "div" => Ok(Instruction::Div {
            rd: parse_register(arg(&args, 0)?)?,
            rs1: parse_register(arg(&args, 1)?)?,
            rs2: parse_register(arg(&args, 2)?)?,
        }),
        "divu" => Ok(Instruction::Divu {
            rd: parse_register(arg(&args, 0)?)?,
            rs1: parse_register(arg(&args, 1)?)?,
            rs2: parse_register(arg(&args, 2)?)?,
        }),
        "rem" => Ok(Instruction::Rem {
            rd: parse_register(arg(&args, 0)?)?,
            rs1: parse_register(arg(&args, 1)?)?,
            rs2: parse_register(arg(&args, 2)?)?,
        }),
        "remu" => Ok(Instruction::Remu {
            rd: parse_register(arg(&args, 0)?)?,
            rs1: parse_register(arg(&args, 1)?)?,
            rs2: parse_register(arg(&args, 2)?)?,
        }),
        "slt" => Ok(Instruction::Slt {
            rd: parse_register(arg(&args, 0)?)?,
            rs1: parse_register(arg(&args, 1)?)?,
            rs2: parse_register(arg(&args, 2)?)?,
        }),
        "sltu" => Ok(Instruction::Sltu {
            rd: parse_register(arg(&args, 0)?)?,
            rs1: parse_register(arg(&args, 1)?)?,
            rs2: parse_register(arg(&args, 2)?)?,
        }),
        "sgt" => Ok(Instruction::Sgt {
            rd: parse_register(arg(&args, 0)?)?,
            rs1: parse_register(arg(&args, 1)?)?,
            rs2: parse_register(arg(&args, 2)?)?,
        }),
        "sgtu" => Ok(Instruction::Sgtu {
            rd: parse_register(arg(&args, 0)?)?,
            rs1: parse_register(arg(&args, 1)?)?,
            rs2: parse_register(arg(&args, 2)?)?,
        }),
        "xori" => Ok(Instruction::Xori {
            rd: parse_register(arg(&args, 0)?)?,
            rs1: parse_register(arg(&args, 1)?)?,
            imm: parse_immediate(arg(&args, 2)?)?,
        }),
        "seqz" => Ok(Instruction::Seqz { rd: parse_register(arg(&args, 0)?)?, rs: parse_register(arg(&args, 1)?)? }),
        "snez" => Ok(Instruction::Snez { rd: parse_register(arg(&args, 0)?)?, rs: parse_register(arg(&args, 1)?)? }),
        "neg" => Ok(Instruction::Neg { rd: parse_register(arg(&args, 0)?)?, rs: parse_register(arg(&args, 1)?)? }),
        "ld" => {
            let (imm, rs1) = parse_memory_operand(arg(&args, 1)?)?;
            Ok(Instruction::Ld { rd: parse_register(arg(&args, 0)?)?, rs1, imm })
        }
        "lbu" => {
            let (imm, rs1) = parse_memory_operand(arg(&args, 1)?)?;
            Ok(Instruction::Lbu { rd: parse_register(arg(&args, 0)?)?, rs1, imm })
        }
        "sb" => {
            let (imm, rs1) = parse_memory_operand(arg(&args, 1)?)?;
            Ok(Instruction::Sb { rs2: parse_register(arg(&args, 0)?)?, rs1, imm })
        }
        "sh" => {
            let (imm, rs1) = parse_memory_operand(arg(&args, 1)?)?;
            Ok(Instruction::Sh { rs2: parse_register(arg(&args, 0)?)?, rs1, imm })
        }
        "sw" => {
            let (imm, rs1) = parse_memory_operand(arg(&args, 1)?)?;
            Ok(Instruction::Sw { rs2: parse_register(arg(&args, 0)?)?, rs1, imm })
        }
        "sd" => {
            let (imm, rs1) = parse_memory_operand(arg(&args, 1)?)?;
            Ok(Instruction::Sd { rs2: parse_register(arg(&args, 0)?)?, rs1, imm })
        }
        "slli" => Ok(Instruction::Slli {
            rd: parse_register(arg(&args, 0)?)?,
            rs1: parse_register(arg(&args, 1)?)?,
            shamt: parse_immediate(arg(&args, 2)?)?,
        }),
        "srli" => Ok(Instruction::Srli {
            rd: parse_register(arg(&args, 0)?)?,
            rs1: parse_register(arg(&args, 1)?)?,
            shamt: parse_immediate(arg(&args, 2)?)?,
        }),
        "li" => Ok(Instruction::Li { rd: parse_register(arg(&args, 0)?)?, imm: parse_li_immediate(arg(&args, 1)?)? }),
        "mv" => Ok(Instruction::Addi { rd: parse_register(arg(&args, 0)?)?, rs1: parse_register(arg(&args, 1)?)?, imm: 0 }),
        "la" => Ok(Instruction::La { rd: parse_register(arg(&args, 0)?)?, label: arg(&args, 1)?.to_string() }),
        "call" => Ok(Instruction::Call { label: arg(&args, 0)?.to_string() }),
        "j" => Ok(Instruction::Jump { label: arg(&args, 0)?.to_string() }),
        "bgt" => Ok(Instruction::Blt {
            rs1: parse_register(arg(&args, 1)?)?,
            rs2: parse_register(arg(&args, 0)?)?,
            label: arg(&args, 2)?.to_string(),
        }),
        "bgez" => Ok(Instruction::Bge { rs1: parse_register(arg(&args, 0)?)?, rs2: 0, label: arg(&args, 1)?.to_string() }),
        "beq" | "bne" | "blt" | "bge" | "bltu" | "bgeu" => {
            let rs1 = parse_register(arg(&args, 0)?)?;
            let rs2 = parse_register(arg(&args, 1)?)?;
            let label = arg(&args, 2)?.to_string();
            match opcode {
                "beq" => Ok(Instruction::Beq { rs1, rs2, label }),
                "bne" => Ok(Instruction::Bne { rs1, rs2, label }),
                "blt" => Ok(Instruction::Blt { rs1, rs2, label }),
                "bge" => Ok(Instruction::Bge { rs1, rs2, label }),
                "bltu" => Ok(Instruction::Bltu { rs1, rs2, label }),
                "bgeu" => Ok(Instruction::Bgeu { rs1, rs2, label }),
                _ => Err(CompileError::new(format!("unsupported branch opcode '{}'", opcode), crate::error::Span::default())),
            }
        }
        "beqz" => Ok(Instruction::Beqz { rs: parse_register(arg(&args, 0)?)?, label: arg(&args, 1)?.to_string() }),
        "bnez" => Ok(Instruction::Bnez { rs: parse_register(arg(&args, 0)?)?, label: arg(&args, 1)?.to_string() }),
        "ret" => Ok(Instruction::Ret),
        "ecall" => Ok(Instruction::Ecall),
        other => Err(CompileError::new(format!("unsupported assembly instruction '{}'", other), crate::error::Span::default())),
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum BranchSizeMode<'a> {
    Conservative,
    Exact(&'a BTreeSet<usize>),
}

impl<'a> BranchSizeMode<'a> {
    fn relaxed_text_branches(self) -> Option<&'a BTreeSet<usize>> {
        match self {
            Self::Conservative => None,
            Self::Exact(branches) => Some(branches),
        }
    }
}

fn branch_target(inst: &Instruction, parsed: &ParsedAssembly, layout: &SectionLayout) -> Result<u64> {
    if let Some((_, _, label, _)) = conditional_branch_parts(inst) {
        parsed.symbol_address(label, layout)
    } else {
        Err(CompileError::new("instruction is not a conditional branch", crate::error::Span::default()))
    }
}

fn conditional_branch_parts(inst: &Instruction) -> Option<(u8, u8, &str, u32)> {
    match inst {
        Instruction::Beq { rs1, rs2, label } => Some((*rs1, *rs2, label.as_str(), 0b000)),
        Instruction::Bne { rs1, rs2, label } => Some((*rs1, *rs2, label.as_str(), 0b001)),
        Instruction::Blt { rs1, rs2, label } => Some((*rs1, *rs2, label.as_str(), 0b100)),
        Instruction::Bge { rs1, rs2, label } => Some((*rs1, *rs2, label.as_str(), 0b101)),
        Instruction::Bltu { rs1, rs2, label } => Some((*rs1, *rs2, label.as_str(), 0b110)),
        Instruction::Bgeu { rs1, rs2, label } => Some((*rs1, *rs2, label.as_str(), 0b111)),
        Instruction::Beqz { rs, label } => Some((*rs, 0, label.as_str(), 0b000)),
        Instruction::Bnez { rs, label } => Some((*rs, 0, label.as_str(), 0b001)),
        _ => None,
    }
}

fn inverse_branch_funct3(funct3: u32) -> Option<u32> {
    match funct3 {
        0b000 => Some(0b001),
        0b001 => Some(0b000),
        0b100 => Some(0b101),
        0b101 => Some(0b100),
        0b110 => Some(0b111),
        0b111 => Some(0b110),
        _ => None,
    }
}

fn encode_instruction(
    out: &mut Vec<u8>,
    inst: &Instruction,
    pc: u64,
    parsed: &ParsedAssembly,
    layout: &SectionLayout,
    relaxed_branch: bool,
) -> Result<()> {
    match inst {
        Instruction::Addi { rd, rs1, imm } => out.extend_from_slice(&encode_i_type(0x13, *rd, 0b000, *rs1, *imm)?.to_le_bytes()),
        Instruction::Add { rd, rs1, rs2 } => {
            out.extend_from_slice(&encode_r_type(0x33, *rd, 0b000, *rs1, *rs2, 0b0000000).to_le_bytes())
        }
        Instruction::Sub { rd, rs1, rs2 } => {
            out.extend_from_slice(&encode_r_type(0x33, *rd, 0b000, *rs1, *rs2, 0b0100000).to_le_bytes())
        }
        Instruction::And { rd, rs1, rs2 } => {
            out.extend_from_slice(&encode_r_type(0x33, *rd, 0b111, *rs1, *rs2, 0b0000000).to_le_bytes())
        }
        Instruction::Andi { rd, rs1, imm } => out.extend_from_slice(&encode_i_type(0x13, *rd, 0b111, *rs1, *imm)?.to_le_bytes()),
        Instruction::Or { rd, rs1, rs2 } => {
            out.extend_from_slice(&encode_r_type(0x33, *rd, 0b110, *rs1, *rs2, 0b0000000).to_le_bytes())
        }
        Instruction::Xor { rd, rs1, rs2 } => {
            out.extend_from_slice(&encode_r_type(0x33, *rd, 0b100, *rs1, *rs2, 0b0000000).to_le_bytes())
        }
        Instruction::Mul { rd, rs1, rs2 } => {
            out.extend_from_slice(&encode_r_type(0x33, *rd, 0b000, *rs1, *rs2, 0b0000001).to_le_bytes())
        }
        Instruction::Div { rd, rs1, rs2 } => {
            out.extend_from_slice(&encode_r_type(0x33, *rd, 0b100, *rs1, *rs2, 0b0000001).to_le_bytes())
        }
        Instruction::Divu { rd, rs1, rs2 } => {
            out.extend_from_slice(&encode_r_type(0x33, *rd, 0b101, *rs1, *rs2, 0b0000001).to_le_bytes())
        }
        Instruction::Rem { rd, rs1, rs2 } => {
            out.extend_from_slice(&encode_r_type(0x33, *rd, 0b110, *rs1, *rs2, 0b0000001).to_le_bytes())
        }
        Instruction::Remu { rd, rs1, rs2 } => {
            out.extend_from_slice(&encode_r_type(0x33, *rd, 0b111, *rs1, *rs2, 0b0000001).to_le_bytes())
        }
        Instruction::Slt { rd, rs1, rs2 } => {
            out.extend_from_slice(&encode_r_type(0x33, *rd, 0b010, *rs1, *rs2, 0b0000000).to_le_bytes())
        }
        Instruction::Sltu { rd, rs1, rs2 } => {
            out.extend_from_slice(&encode_r_type(0x33, *rd, 0b011, *rs1, *rs2, 0b0000000).to_le_bytes())
        }
        Instruction::Sgt { rd, rs1, rs2 } => {
            out.extend_from_slice(&encode_r_type(0x33, *rd, 0b010, *rs2, *rs1, 0b0000000).to_le_bytes())
        }
        Instruction::Sgtu { rd, rs1, rs2 } => {
            out.extend_from_slice(&encode_r_type(0x33, *rd, 0b011, *rs2, *rs1, 0b0000000).to_le_bytes())
        }
        Instruction::Xori { rd, rs1, imm } => out.extend_from_slice(&encode_i_type(0x13, *rd, 0b100, *rs1, *imm)?.to_le_bytes()),
        Instruction::Seqz { rd, rs } => out.extend_from_slice(&encode_i_type(0x13, *rd, 0b011, *rs, 1)?.to_le_bytes()),
        Instruction::Snez { rd, rs } => out.extend_from_slice(&encode_r_type(0x33, *rd, 0b011, 0, *rs, 0b0000000).to_le_bytes()),
        Instruction::Neg { rd, rs } => out.extend_from_slice(&encode_r_type(0x33, *rd, 0b000, 0, *rs, 0b0100000).to_le_bytes()),
        Instruction::Ld { rd, rs1, imm } => out.extend_from_slice(&encode_i_type(0x03, *rd, 0b011, *rs1, *imm)?.to_le_bytes()),
        Instruction::Lbu { rd, rs1, imm } => out.extend_from_slice(&encode_i_type(0x03, *rd, 0b100, *rs1, *imm)?.to_le_bytes()),
        Instruction::Sb { rs2, rs1, imm } => out.extend_from_slice(&encode_s_type(0x23, 0b000, *rs1, *rs2, *imm)?.to_le_bytes()),
        Instruction::Sh { rs2, rs1, imm } => out.extend_from_slice(&encode_s_type(0x23, 0b001, *rs1, *rs2, *imm)?.to_le_bytes()),
        Instruction::Sw { rs2, rs1, imm } => out.extend_from_slice(&encode_s_type(0x23, 0b010, *rs1, *rs2, *imm)?.to_le_bytes()),
        Instruction::Sd { rs2, rs1, imm } => out.extend_from_slice(&encode_s_type(0x23, 0b011, *rs1, *rs2, *imm)?.to_le_bytes()),
        Instruction::Slli { rd, rs1, shamt } => {
            if !(0..=63).contains(shamt) {
                return Err(CompileError::new("slli shift amount must be in 0..=63", crate::error::Span::default()));
            }
            out.extend_from_slice(&encode_i_type(0x13, *rd, 0b001, *rs1, *shamt)?.to_le_bytes());
        }
        Instruction::Srli { rd, rs1, shamt } => {
            if !(0..=63).contains(shamt) {
                return Err(CompileError::new("srli shift amount must be in 0..=63", crate::error::Span::default()));
            }
            out.extend_from_slice(&encode_i_type(0x13, *rd, 0b101, *rs1, *shamt)?.to_le_bytes());
        }
        Instruction::Li { rd, imm } => encode_li_sequence(out, *rd, *imm)?,
        Instruction::La { rd, label } => encode_address_sequence(out, *rd, pc, parsed.symbol_address(label, layout)?)?,
        Instruction::Call { label } => {
            let target = parsed.symbol_address(label, layout)?;
            encode_call_sequence(out, pc, target)?;
        }
        Instruction::Jump { label } => {
            let target = parsed.symbol_address(label, layout)?;
            if relaxed_branch {
                encode_long_jump_sequence(out, pc, target)?;
            } else {
                out.extend_from_slice(&encode_j_type(0x6f, 0, relative_offset(pc, target)?)?.to_le_bytes());
            }
        }
        Instruction::Beq { .. }
        | Instruction::Bne { .. }
        | Instruction::Blt { .. }
        | Instruction::Bge { .. }
        | Instruction::Bltu { .. }
        | Instruction::Bgeu { .. }
        | Instruction::Beqz { .. }
        | Instruction::Bnez { .. } => {
            let Some((rs1, rs2, label, funct3)) = conditional_branch_parts(inst) else {
                return Err(CompileError::new("malformed conditional branch instruction", crate::error::Span::default()));
            };
            let target = parsed.symbol_address(label, layout)?;
            if relaxed_branch {
                let inverse = inverse_branch_funct3(funct3)
                    .ok_or_else(|| CompileError::new("unsupported conditional branch function", crate::error::Span::default()))?;
                out.extend_from_slice(&encode_b_type(0x63, inverse, rs1, rs2, 12)?.to_le_bytes());
                encode_long_jump_sequence(out, pc + 4, target)?;
            } else {
                out.extend_from_slice(&encode_b_type(0x63, funct3, rs1, rs2, relative_offset(pc, target)?)?.to_le_bytes());
            }
        }
        Instruction::Ret => out.extend_from_slice(&encode_i_type(0x67, 0, 0b000, 1, 0)?.to_le_bytes()),
        Instruction::Ecall => out.extend_from_slice(&encode_ecall().to_le_bytes()),
    }
    Ok(())
}

pub(crate) fn encode_li_sequence(out: &mut Vec<u8>, rd: u8, imm: i128) -> Result<()> {
    if let Some(signed) = li_signed_i64(imm) {
        if li_fits_lui_addi_rv64(signed) {
            let (hi, lo) = split_hi_lo(signed)?;
            out.extend_from_slice(&encode_u_type(0x37, rd, hi).to_le_bytes());
            out.extend_from_slice(&encode_i_type(0x13, rd, 0b000, rd, lo)?.to_le_bytes());
            return Ok(());
        }
    }
    encode_large_li_sequence(out, rd, li_bits(imm)?)
}

fn encode_large_li_sequence(out: &mut Vec<u8>, rd: u8, bits: u64) -> Result<()> {
    let bytes = bits.to_be_bytes();
    out.extend_from_slice(&encode_i_type(0x13, rd, 0b000, 0, i64::from(bytes[0]))?.to_le_bytes());
    for byte in bytes.iter().skip(1) {
        out.extend_from_slice(&encode_i_type(0x13, rd, 0b001, rd, 8)?.to_le_bytes());
        out.extend_from_slice(&encode_i_type(0x13, rd, 0b000, rd, i64::from(*byte))?.to_le_bytes());
    }
    Ok(())
}

fn li_signed_i64(imm: i128) -> Option<i64> {
    i64::try_from(imm).ok()
}

fn li_bits(imm: i128) -> Result<u64> {
    if imm < i128::from(i64::MIN) || imm > i128::from(u64::MAX) {
        return Err(CompileError::new(format!("li immediate '{}' does not fit 64 bits", imm), crate::error::Span::default()));
    }
    if imm < 0 {
        Ok((imm as i64) as u64)
    } else {
        Ok(imm as u64)
    }
}

fn encode_address_sequence(out: &mut Vec<u8>, rd: u8, pc: u64, target: u64) -> Result<()> {
    let (hi, lo) = split_hi_lo(relative_offset(pc, target)?)?;
    out.extend_from_slice(&encode_u_type(0x17, rd, hi).to_le_bytes());
    out.extend_from_slice(&encode_i_type(0x13, rd, 0b000, rd, lo)?.to_le_bytes());
    Ok(())
}

fn encode_call_sequence(out: &mut Vec<u8>, pc: u64, target: u64) -> Result<()> {
    let offset = relative_offset(pc, target)?;
    ensure_uncompressed_instruction_alignment(offset, "call target")?;
    let (hi, lo) = split_hi_lo(offset)?;
    out.extend_from_slice(&encode_u_type(0x17, 1, hi).to_le_bytes());
    out.extend_from_slice(&encode_i_type(0x67, 1, 0b000, 1, lo)?.to_le_bytes());
    Ok(())
}

fn encode_long_jump_sequence(out: &mut Vec<u8>, pc: u64, target: u64) -> Result<()> {
    // Relaxed long jumps use t6 as a dedicated assembler scratch. Codegen's
    // stack-machine contract must not keep t6 live across labels that can be
    // reached by a branch or jump relaxation.
    let scratch = 31;
    let offset = relative_offset(pc, target)?;
    ensure_uncompressed_instruction_alignment(offset, "jump target")?;
    let (hi, lo) = split_hi_lo(offset)?;
    out.extend_from_slice(&encode_u_type(0x17, scratch, hi).to_le_bytes());
    out.extend_from_slice(&encode_i_type(0x67, 0, 0b000, scratch, lo)?.to_le_bytes());
    Ok(())
}

fn op_size(op: &AsmOp, current_offset: usize, section: SectionKind, op_index: usize, branch_size_mode: BranchSizeMode<'_>) -> usize {
    match op {
        AsmOp::Label(_) => 0,
        AsmOp::Instruction(Instruction::Li { imm, .. }) => li_sequence_size(*imm),
        AsmOp::Instruction(Instruction::La { .. }) => 8,
        AsmOp::Instruction(Instruction::Call { .. }) => 8,
        AsmOp::Instruction(Instruction::Jump { .. }) => match branch_size_mode {
            BranchSizeMode::Conservative => 8,
            BranchSizeMode::Exact(relaxed) if section == SectionKind::Text && relaxed.contains(&op_index) => 8,
            BranchSizeMode::Exact(_) => 4,
        },
        AsmOp::Instruction(
            Instruction::Beq { .. }
            | Instruction::Bne { .. }
            | Instruction::Blt { .. }
            | Instruction::Bge { .. }
            | Instruction::Bltu { .. }
            | Instruction::Bgeu { .. }
            | Instruction::Beqz { .. }
            | Instruction::Bnez { .. },
        ) => match branch_size_mode {
            BranchSizeMode::Conservative => 12,
            BranchSizeMode::Exact(relaxed) if section == SectionKind::Text && relaxed.contains(&op_index) => 12,
            BranchSizeMode::Exact(_) => 4,
        },
        AsmOp::Instruction(_) => 4,
        AsmOp::Word(_) => 4,
        AsmOp::Byte(_) => 1,
        AsmOp::Ascii(bytes) => bytes.len(),
        AsmOp::Align(bytes) => padding_for(current_offset, *bytes),
    }
}

fn li_sequence_size(imm: i128) -> usize {
    if li_signed_i64(imm).is_some_and(li_fits_lui_addi_rv64) {
        8
    } else {
        60
    }
}

fn start_trampoline_size() -> usize {
    li_sequence_size(i128::from(CKB_SCRIPT_STACK_TOP)) + 8 + li_sequence_size(i128::from(EXIT_SYSCALL_NUMBER)) + 4
}

fn write_elf_header(out: &mut [u8], entry: u64, program_header_count: u16) -> Result<()> {
    if out.len() != ELF_HEADER_SIZE {
        return Err(CompileError::new("invalid ELF header buffer size", crate::error::Span::default()));
    }
    out.fill(0);
    out[0..4].copy_from_slice(b"\x7fELF");
    out[4] = 2;
    out[5] = 1;
    out[6] = 1;
    out[16..18].copy_from_slice(&2u16.to_le_bytes());
    out[18..20].copy_from_slice(&243u16.to_le_bytes());
    out[20..24].copy_from_slice(&1u32.to_le_bytes());
    out[24..32].copy_from_slice(&entry.to_le_bytes());
    out[32..40].copy_from_slice(&(ELF_HEADER_SIZE as u64).to_le_bytes());
    out[40..48].copy_from_slice(&0u64.to_le_bytes());
    out[48..52].copy_from_slice(&0u32.to_le_bytes());
    out[52..54].copy_from_slice(&(ELF_HEADER_SIZE as u16).to_le_bytes());
    out[54..56].copy_from_slice(&(ELF_PROGRAM_HEADER_SIZE as u16).to_le_bytes());
    out[56..58].copy_from_slice(&program_header_count.to_le_bytes());
    Ok(())
}

fn write_program_header(out: &mut [u8], flags: u32, offset: u64, vaddr: u64, file_size: u64, memory_size: u64) -> Result<()> {
    if out.len() != ELF_PROGRAM_HEADER_SIZE {
        return Err(CompileError::new("invalid ELF program header buffer size", crate::error::Span::default()));
    }
    out.fill(0);
    out[0..4].copy_from_slice(&1u32.to_le_bytes());
    out[4..8].copy_from_slice(&flags.to_le_bytes());
    out[8..16].copy_from_slice(&offset.to_le_bytes());
    out[16..24].copy_from_slice(&vaddr.to_le_bytes());
    out[24..32].copy_from_slice(&vaddr.to_le_bytes());
    out[32..40].copy_from_slice(&file_size.to_le_bytes());
    out[40..48].copy_from_slice(&memory_size.to_le_bytes());
    out[48..56].copy_from_slice(&(ELF_SEGMENT_ALIGN as u64).to_le_bytes());
    Ok(())
}

pub(crate) fn strip_comment(line: &str) -> Option<&str> {
    let mut in_string = false;
    let mut escape = false;
    for (idx, ch) in line.char_indices() {
        match ch {
            '"' if !escape => in_string = !in_string,
            '#' if !in_string => return Some(line[..idx].trim()),
            '\\' if in_string => {
                escape = !escape;
                continue;
            }
            _ => {}
        }
        escape = false;
    }
    let trimmed = line.trim();
    (!trimmed.is_empty()).then_some(trimmed)
}

fn parse_ascii_literal(value: &str) -> Result<Vec<u8>> {
    let Some(inner) = value.strip_prefix('"').and_then(|value| value.strip_suffix('"')) else {
        return Err(CompileError::new(format!("invalid .ascii literal '{}'", value), crate::error::Span::default()));
    };

    let mut out = Vec::new();
    let mut chars = inner.chars();
    while let Some(ch) = chars.next() {
        if ch != '\\' {
            out.extend_from_slice(ch.to_string().as_bytes());
            continue;
        }

        let escaped = chars
            .next()
            .ok_or_else(|| CompileError::new("unterminated escape sequence in .ascii literal", crate::error::Span::default()))?;
        match escaped {
            'n' => out.push(b'\n'),
            'r' => out.push(b'\r'),
            't' => out.push(b'\t'),
            '\\' => out.push(b'\\'),
            '"' => out.push(b'"'),
            'x' => {
                let hi = chars
                    .next()
                    .ok_or_else(|| CompileError::new("incomplete hex escape in .ascii literal", crate::error::Span::default()))?;
                let lo = chars
                    .next()
                    .ok_or_else(|| CompileError::new("incomplete hex escape in .ascii literal", crate::error::Span::default()))?;
                let hex = format!("{}{}", hi, lo);
                let byte = u8::from_str_radix(&hex, 16)
                    .map_err(|_| CompileError::new(format!("invalid hex escape '\\x{}'", hex), crate::error::Span::default()))?;
                out.push(byte);
            }
            other => {
                return Err(CompileError::new(
                    format!("unsupported escape sequence '\\{}' in .ascii literal", other),
                    crate::error::Span::default(),
                ));
            }
        }
    }

    Ok(out)
}

fn parse_memory_operand(value: &str) -> Result<(i64, u8)> {
    let open = value
        .find('(')
        .ok_or_else(|| CompileError::new(format!("invalid memory operand '{}'", value), crate::error::Span::default()))?;
    let close = value
        .rfind(')')
        .ok_or_else(|| CompileError::new(format!("invalid memory operand '{}'", value), crate::error::Span::default()))?;
    let imm = parse_immediate(value[..open].trim())?;
    let rs1 = parse_register(value[open + 1..close].trim())?;
    Ok((imm, rs1))
}

pub(crate) fn memory_operand_offset_and_base(value: &str) -> Option<(i64, &str)> {
    let open = value.find('(')?;
    let close = value.rfind(')')?;
    let offset = parse_immediate(value[..open].trim()).ok()?;
    let base = value[open + 1..close].trim();
    (!base.is_empty()).then_some((offset, base))
}

pub(crate) fn small_signed_immediate(value: i64) -> bool {
    (-2048..=2047).contains(&value)
}

pub(crate) fn scratch_register_avoiding(registers: &[&str]) -> &'static str {
    for candidate in ["t6", "t5", "t3", "t2", "t1", "t0"] {
        let Ok(candidate_id) = parse_register(candidate) else {
            continue;
        };
        if registers.iter().all(|register| parse_register(register).ok() != Some(candidate_id)) {
            return candidate;
        }
    }
    "t6"
}

pub(crate) fn parse_register(name: &str) -> Result<u8> {
    let reg = match name {
        "zero" | "x0" => 0,
        "ra" | "x1" => 1,
        "sp" | "x2" => 2,
        "gp" | "x3" => 3,
        "tp" | "x4" => 4,
        "t0" | "x5" => 5,
        "t1" | "x6" => 6,
        "t2" | "x7" => 7,
        "s0" | "fp" | "x8" => 8,
        "s1" | "x9" => 9,
        "a0" | "x10" => 10,
        "a1" | "x11" => 11,
        "a2" | "x12" => 12,
        "a3" | "x13" => 13,
        "a4" | "x14" => 14,
        "a5" | "x15" => 15,
        "a6" | "x16" => 16,
        "a7" | "x17" => 17,
        "s2" | "x18" => 18,
        "s3" | "x19" => 19,
        "s4" | "x20" => 20,
        "s5" | "x21" => 21,
        "s6" | "x22" => 22,
        "s7" | "x23" => 23,
        "s8" | "x24" => 24,
        "s9" | "x25" => 25,
        "s10" | "x26" => 26,
        "s11" | "x27" => 27,
        "t3" | "x28" => 28,
        "t4" | "x29" => 29,
        "t5" | "x30" => 30,
        "t6" | "x31" => 31,
        other => return Err(CompileError::new(format!("unknown register '{}'", other), crate::error::Span::default())),
    };
    Ok(reg)
}

pub(crate) fn parse_immediate(value: &str) -> Result<i64> {
    if let Some(hex) = value.strip_prefix("-0x") {
        return i64::from_str_radix(hex, 16)
            .map(|value| -value)
            .map_err(|_| CompileError::new(format!("invalid immediate '{}'", value), crate::error::Span::default()));
    }
    if let Some(hex) = value.strip_prefix("0x") {
        return i64::from_str_radix(hex, 16)
            .map_err(|_| CompileError::new(format!("invalid immediate '{}'", value), crate::error::Span::default()));
    }
    value.parse::<i64>().map_err(|_| CompileError::new(format!("invalid immediate '{}'", value), crate::error::Span::default()))
}

fn parse_li_immediate(value: &str) -> Result<i128> {
    if let Some(hex) = value.strip_prefix("-0x") {
        return i128::from_str_radix(hex, 16)
            .map(|value| -value)
            .map_err(|_| CompileError::new(format!("invalid immediate '{}'", value), crate::error::Span::default()));
    }
    if let Some(hex) = value.strip_prefix("0x") {
        let parsed = u128::from_str_radix(hex, 16)
            .map_err(|_| CompileError::new(format!("invalid immediate '{}'", value), crate::error::Span::default()))?;
        if parsed <= u128::from(u64::MAX) {
            return Ok(parsed as i128);
        }
        return Err(CompileError::new(format!("li immediate '{}' does not fit 64 bits", value), crate::error::Span::default()));
    }
    if value.starts_with('-') {
        value.parse::<i128>().map_err(|_| CompileError::new(format!("invalid immediate '{}'", value), crate::error::Span::default()))
    } else {
        value
            .parse::<u128>()
            .map_err(|_| CompileError::new(format!("invalid immediate '{}'", value), crate::error::Span::default()))
            .and_then(|parsed| {
                if parsed <= u128::from(u64::MAX) {
                    Ok(parsed as i128)
                } else {
                    Err(CompileError::new(format!("li immediate '{}' does not fit 64 bits", value), crate::error::Span::default()))
                }
            })
    }
}

fn arg(args: &[String], index: usize) -> Result<&str> {
    args.get(index)
        .map(|value| value.as_str())
        .ok_or_else(|| CompileError::new("malformed assembly instruction", crate::error::Span::default()))
}

fn encode_r_type(opcode: u32, rd: u8, funct3: u32, rs1: u8, rs2: u8, funct7: u32) -> u32 {
    (funct7 << 25) | ((rs2 as u32) << 20) | ((rs1 as u32) << 15) | (funct3 << 12) | ((rd as u32) << 7) | opcode
}

pub(crate) fn encode_i_type(opcode: u32, rd: u8, funct3: u32, rs1: u8, imm: i64) -> Result<u32> {
    let imm = encode_signed_bits(imm, 12)?;
    Ok((imm << 20) | ((rs1 as u32) << 15) | (funct3 << 12) | ((rd as u32) << 7) | opcode)
}

pub(crate) fn encode_s_type(opcode: u32, funct3: u32, rs1: u8, rs2: u8, imm: i64) -> Result<u32> {
    let imm = encode_signed_bits(imm, 12)?;
    let imm_lo = imm & 0x1f;
    let imm_hi = (imm >> 5) & 0x7f;
    Ok((imm_hi << 25) | ((rs2 as u32) << 20) | ((rs1 as u32) << 15) | (funct3 << 12) | (imm_lo << 7) | opcode)
}

fn encode_b_type(opcode: u32, funct3: u32, rs1: u8, rs2: u8, imm: i64) -> Result<u32> {
    ensure_uncompressed_instruction_alignment(imm, "branch target")?;
    let imm = encode_signed_bits(imm, 13)?;
    let bit12 = (imm >> 12) & 0x1;
    let bits10_5 = (imm >> 5) & 0x3f;
    let bits4_1 = (imm >> 1) & 0xf;
    let bit11 = (imm >> 11) & 0x1;
    Ok((bit12 << 31)
        | (bits10_5 << 25)
        | ((rs2 as u32) << 20)
        | ((rs1 as u32) << 15)
        | (funct3 << 12)
        | (bits4_1 << 8)
        | (bit11 << 7)
        | opcode)
}

fn encode_u_type(opcode: u32, rd: u8, imm: i64) -> u32 {
    (((imm as i32 as u32) & 0x000f_ffff) << 12) | ((rd as u32) << 7) | opcode
}

fn encode_j_type(opcode: u32, rd: u8, imm: i64) -> Result<u32> {
    ensure_uncompressed_instruction_alignment(imm, "jump target")?;
    let imm = encode_signed_bits(imm, 21)?;
    let bit20 = (imm >> 20) & 0x1;
    let bits10_1 = (imm >> 1) & 0x3ff;
    let bit11 = (imm >> 11) & 0x1;
    let bits19_12 = (imm >> 12) & 0xff;
    Ok((bit20 << 31) | (bits10_1 << 21) | (bit11 << 20) | (bits19_12 << 12) | ((rd as u32) << 7) | opcode)
}

pub(crate) fn encode_ecall() -> u32 {
    0x0000_0073
}

fn encode_signed_bits(value: i64, bits: u32) -> Result<u32> {
    if !signed_bits_fit(value, bits) {
        return Err(CompileError::new(
            format!("immediate '{}' does not fit {}-bit signed field", value, bits),
            crate::error::Span::default(),
        ));
    }
    Ok((value as i32 as u32) & ((1u32 << bits) - 1))
}

pub(crate) fn signed_bits_fit(value: i64, bits: u32) -> bool {
    match bits {
        0 => false,
        1..=63 => {
            let min = -(1i64 << (bits - 1));
            let max = (1i64 << (bits - 1)) - 1;
            value >= min && value <= max
        }
        64 => true,
        _ => false,
    }
}

fn split_hi_lo(value: i64) -> Result<(i64, i64)> {
    if !li_fits_lui_addi_rv64(value) {
        return Err(CompileError::new(
            format!("value '{}' is outside the supported RV64 LUI/ADDI immediate range", value),
            crate::error::Span::default(),
        ));
    }
    let hi = lui_addi_hi20(value).ok_or_else(|| {
        CompileError::new(
            format!("value '{}' is outside the supported RV64 LUI/ADDI immediate range", value),
            crate::error::Span::default(),
        )
    })?;
    let lo = value - (hi << 12);
    if !(-2048..=2047).contains(&lo) {
        return Err(CompileError::new(format!("low immediate '{}' is out of range after split", lo), crate::error::Span::default()));
    }
    Ok((hi, lo))
}

fn li_fits_lui_addi_rv64(value: i64) -> bool {
    lui_addi_hi20(value).is_some_and(|hi| (-2048..=2047).contains(&(value - (hi << 12))))
}

fn lui_addi_hi20(value: i64) -> Option<i64> {
    if !(i32::MIN as i64..=i32::MAX as i64).contains(&value) {
        return None;
    }
    let hi_bits = (((value as i32).wrapping_add(0x800) as u32) >> 12) & 0x000f_ffff;
    Some(sign_extend_u32_to_i64(hi_bits, 20))
}

fn sign_extend_u32_to_i64(value: u32, bits: u32) -> i64 {
    let shift = 32 - bits;
    ((value << shift) as i32 >> shift) as i64
}

fn ensure_uncompressed_instruction_alignment(offset: i64, context: &str) -> Result<()> {
    if offset % 4 != 0 {
        return Err(CompileError::new(format!("{} is not 4-byte aligned", context), crate::error::Span::default()));
    }
    Ok(())
}

fn relative_offset(pc: u64, target: u64) -> Result<i64> {
    i64::try_from(target as i128 - pc as i128)
        .map_err(|_| CompileError::new("relative offset overflowed i64", crate::error::Span::default()))
}

pub(crate) fn align_up(value: usize, align: usize) -> usize {
    if align <= 1 {
        return value;
    }
    (value + align - 1) & !(align - 1)
}

pub(crate) fn align_frame(value: usize) -> usize {
    align_up(value.max(16), 16)
}

pub(crate) fn is_min_call(func: &str) -> bool {
    matches!(func, "min" | "math_min" | "__math_min")
}

pub(crate) fn is_runtime_header_u64_call(func: &str) -> bool {
    matches!(
        func,
        "__env_current_timepoint"
            | "__ckb_header_epoch_number"
            | "__ckb_header_epoch_start_block_number"
            | "__ckb_header_epoch_length"
            | "__ckb_input_since"
    )
}

pub(crate) fn ckb_source_name(source: u64) -> &'static str {
    match source {
        CKB_SOURCE_INPUT => "Input",
        CKB_SOURCE_OUTPUT => "Output",
        CKB_SOURCE_CELL_DEP => "CellDep",
        CKB_SOURCE_HEADER_DEP => "HeaderDep",
        CKB_SOURCE_GROUP_INPUT => "GroupInput",
        CKB_SOURCE_GROUP_OUTPUT => "GroupOutput",
        CKB_SOURCE_GROUP_CELL_DEP => "GroupCellDep",
        CKB_SOURCE_GROUP_HEADER_DEP => "GroupHeaderDep",
        _ => "Unknown",
    }
}

fn padding_for(offset: usize, align: usize) -> usize {
    align_up(offset, align) - offset
}

fn pad_to_alignment(out: &mut Vec<u8>, align: usize) {
    let pad = padding_for(out.len(), align);
    out.resize(out.len() + pad, 0);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn u64_le(bytes: &[u8], start: usize) -> u64 {
        u64::from_le_bytes(bytes[start..start + 8].try_into().expect("u64 field"))
    }

    fn u32_le(bytes: &[u8], start: usize) -> u32 {
        u32::from_le_bytes(bytes[start..start + 4].try_into().expect("u32 field"))
    }

    #[test]
    fn strict_audit_internal_assembler_oracle_for_core_instruction_bytes() {
        assert_eq!(encode_i_type(0x13, 10, 0b000, 0, 1).unwrap().to_le_bytes(), [0x13, 0x05, 0x10, 0x00]);
        assert_eq!(encode_i_type(0x67, 0, 0b000, 1, 0).unwrap().to_le_bytes(), [0x67, 0x80, 0x00, 0x00]);
        assert_eq!(encode_ecall().to_le_bytes(), [0x73, 0x00, 0x00, 0x00]);
        assert_eq!(encode_j_type(0x6f, 0, 8).unwrap().to_le_bytes(), [0x6f, 0x00, 0x80, 0x00]);
        assert_eq!(encode_b_type(0x63, 0b000, 0, 0, 8).unwrap().to_le_bytes(), [0x63, 0x04, 0x00, 0x00]);
    }

    #[test]
    fn strict_audit_riscv_immediate_boundaries_are_enforced() {
        assert!(encode_i_type(0x13, 1, 0, 0, -2048).is_ok());
        assert!(encode_i_type(0x13, 1, 0, 0, 2047).is_ok());
        assert!(encode_i_type(0x13, 1, 0, 0, -2049).is_err());
        assert!(encode_i_type(0x13, 1, 0, 0, 2048).is_err());

        assert!(encode_b_type(0x63, 0, 0, 0, -4096).is_ok());
        assert!(encode_b_type(0x63, 0, 0, 0, 4092).is_ok());
        assert!(encode_b_type(0x63, 0, 0, 0, -4098).is_err());
        assert!(encode_b_type(0x63, 0, 0, 0, 4096).is_err());
        assert!(encode_b_type(0x63, 0, 0, 0, 2).is_err());
        assert!(encode_b_type(0x63, 0, 0, 0, 3).is_err());

        assert!(encode_j_type(0x6f, 0, -1_048_576).is_ok());
        assert!(encode_j_type(0x6f, 0, 1_048_572).is_ok());
        assert!(encode_j_type(0x6f, 0, -1_048_578).is_err());
        assert!(encode_j_type(0x6f, 0, 1_048_576).is_err());
        assert!(encode_j_type(0x6f, 0, 2).is_err());
        assert!(encode_j_type(0x6f, 0, 3).is_err());
    }

    #[test]
    fn strict_audit_li_split_handles_negative_32_bit_boundaries() {
        assert!(li_fits_lui_addi_rv64(i32::MIN as i64));
        assert_eq!(split_hi_lo(i32::MIN as i64).unwrap(), (-0x80000, 0));
        assert_eq!(split_hi_lo(-2_147_481_600).unwrap(), (-0x7ffff, -2048));
        assert_eq!(split_hi_lo(-2049).unwrap(), (-1, 2047));

        let mut bytes = Vec::new();
        encode_li_sequence(&mut bytes, 10, -2_147_481_600).unwrap();
        assert_eq!(bytes.len(), 8, "negative 32-bit boundary should use LUI/ADDI");

        assert!(!li_fits_lui_addi_rv64(2_147_481_600));
    }

    #[test]
    fn strict_audit_elf_header_and_segments_are_internally_consistent() {
        let lines = vec![
            ".section .text".to_string(),
            ".global entry".to_string(),
            "entry:".to_string(),
            "li a0, 0".to_string(),
            "ret".to_string(),
            ".section .rodata".to_string(),
            ".align 3".to_string(),
            "payload:".to_string(),
            ".byte 1".to_string(),
        ];

        let elf = assemble_elf_internal(&lines).expect("minimal ELF should assemble");
        assert_eq!(&elf[..4], b"\x7fELF");
        assert_eq!(u64_le(&elf, 24), ELF_BASE_ADDR);
        assert_eq!(u64_le(&elf, 32), ELF_HEADER_SIZE as u64);

        let ph = ELF_HEADER_SIZE;
        assert_eq!(u32_le(&elf, ph), 1, "single load program header");
        assert_eq!(u32_le(&elf, ph + 4), 5, "load segment must be RX");
        let file_offset = u64_le(&elf, ph + 8) as usize;
        let file_size = u64_le(&elf, ph + 32) as usize;
        assert_eq!(file_offset, 0);
        assert_eq!(file_size, elf.len());

        let plan = MachineLayoutPlan::build(&lines).unwrap();
        let segment_file_offset = align_up(ELF_HEADER_SIZE + ELF_PROGRAM_HEADER_SIZE, ELF_SEGMENT_ALIGN);
        let rodata_start = segment_file_offset + plan.layout.rodata_offset().unwrap();
        assert!(rodata_start < elf.len(), "rodata should be inside the load segment");
        assert_eq!(elf[rodata_start], 1);
    }
}
