//! Parse once, then freeze one address/padding plan for bytes and evidence.
//! Branch decisions start with the previous conservative policy. Subsequent
//! passes only promote short branches, so alignment cannot cause oscillation.
use super::*;

impl ParsedAssembly {
    fn parse(lines: &[String]) -> Result<Self> {
        let mut parsed = Self::default();
        let mut section = SectionKind::Text;
        for (line_index, line) in lines.iter().enumerate() {
            if let Some(marker) = line.trim().strip_prefix("# cellscript entry abi: ")
                && let Some(entry) = marker.strip_suffix(" requires-explicit-parameter-abi")
            {
                parsed.explicit_parameter_entries.insert(entry.to_string());
            }
            let Some(clean) = strip_comment(line).filter(|clean| !clean.is_empty()) else { continue };
            let provenance = GeneratedAssemblyProvenance {
                line: line_index + 1,
                text: clean.to_string(),
                section,
                op_index: if section == SectionKind::Text { parsed.text_ops.len() } else { parsed.rodata_ops.len() },
            };
            if let Some(next) = parse_section_directive(clean).map_err(|error| provenance.attach(error))? {
                section = next;
                continue;
            }
            if clean.starts_with(".option ") || clean.starts_with(".type ") {
                continue;
            }
            if let Some(symbol) = clean.strip_prefix(".global ") {
                let symbol = symbol.trim();
                if symbol.is_empty() || symbol.chars().any(char::is_whitespace) || symbol.contains(',') {
                    return Err(provenance.attach(CompileError::without_span(".global requires exactly one symbol")));
                }
                parsed.globals.insert(symbol.to_string());
                continue;
            }
            let op = if let Some(label) = clean.strip_suffix(':') {
                let label = label.trim().to_string();
                if label.is_empty() || label.chars().any(char::is_whitespace) {
                    return Err(provenance.attach(CompileError::without_span("invalid empty or whitespace-containing assembly label")));
                }
                if let Some(previous) = parsed.symbols.get(&label) {
                    return Err(provenance.attach(CompileError::without_span(format!(
                        "duplicate assembly label '{}' (first defined at generated assembly line {})",
                        label, previous.line
                    ))));
                }
                parsed.symbols.insert(label.clone(), SymbolDef { section, offset: 0, line: line_index + 1 });
                AsmOp::Label(label)
            } else {
                parse_asm_op(clean).map_err(|error| provenance.attach(error))?
            };
            let immediate_plan = match op {
                AsmOp::Instruction(Instruction::Li { imm, .. }) => {
                    Some(immediate::plan(imm).map_err(|error| provenance.attach(error))?)
                }
                _ => None,
            };
            let located = LocatedAsmOp { op, provenance, immediate_plan };
            if section == SectionKind::Text {
                parsed.text_ops.push(located);
            } else {
                parsed.rodata_ops.push(located);
            }
        }
        let entries = parsed.text_ops.iter().filter_map(|op| match &op.op {
            AsmOp::Label(label) if parsed.globals.contains(label) => Some(label),
            _ => None,
        });
        parsed.entry_label = entries.clone().find(|label| !label.starts_with("__")).or_else(|| entries.clone().next()).cloned();
        Ok(parsed)
    }

    fn validate_calls(&self, production: bool) -> Result<()> {
        let calls = self.text_ops.iter().chain(&self.rodata_ops).filter_map(|op| match &op.op {
            AsmOp::Instruction(Instruction::Call { label }) => Some((label, &op.provenance)),
            _ => None,
        });
        let missing = calls.filter(|(target, _)| !self.symbols.contains_key(*target)).collect::<BTreeMap<_, _>>();
        if let Some((target, provenance)) = missing.first_key_value() {
            let error = if production {
                CompileError::without_span(format!(
                    "unresolved call target(s) in generated assembly: {}; production ELF emission requires all call targets to be lowered",
                    missing.keys().map(|target| target.as_str()).collect::<Vec<_>>().join(", ")
                ))
                .with_code(crate::error::DiagnosticCode::E2200)
            } else {
                CompileError::without_span(format!("unknown assembly label '{}'", target))
            };
            return Err(provenance.attach(error));
        }
        Ok(())
    }

    fn place(&mut self, mode: BranchSizeMode<'_>) -> Result<SectionLayout> {
        let (text_size, text_layouts) = place_ops(&self.text_ops, SectionKind::Text, mode, &mut self.symbols)?;
        let rodata_alignment = self
            .rodata_ops
            .iter()
            .filter_map(|op| match op.op {
                AsmOp::Align(bytes) => Some(bytes),
                _ => None,
            })
            .max()
            .unwrap_or(8)
            .max(8);
        let layout = SectionLayout::for_text_user_size(text_size, rodata_alignment)?;
        let (rodata_size, rodata_layouts) = place_ops(&self.rodata_ops, SectionKind::Rodata, mode, &mut self.symbols)?;
        // Both sections share one load segment. Bound their combined extent
        // before branch/symbol address calculations, not only before allocation.
        checked_layout_add(layout.rodata_offset()?, rodata_size)?;
        self.text_size = text_size;
        self.rodata_size = rodata_size;
        self.text_layouts = text_layouts;
        self.rodata_layouts = rodata_layouts;
        Ok(layout)
    }

    fn required_long_branches(&self, layout: &SectionLayout) -> Result<BTreeSet<usize>> {
        let mut required = BTreeSet::new();
        for placed in &self.text_layouts {
            let op = &self.text_ops[placed.op_index];
            if let AsmOp::Instruction(inst) = &op.op
                && conditional_branch_parts(inst).is_some()
            {
                let pc = layout.text_user_base.checked_add(placed.offset as u64).ok_or_else(layout_overflow)?;
                let target = branch_target(inst, self, layout).map_err(|error| op.provenance.attach(error))?;
                let delta = relative_offset(pc, target).map_err(|error| op.provenance.attach(error))?;
                if !signed_bits_fit(delta, 13) {
                    required.insert(placed.op_index);
                }
            }
        }
        Ok(required)
    }

    fn validate_final_branches(&self, layout: &SectionLayout) -> Result<()> {
        for placed in &self.text_layouts {
            let op = &self.text_ops[placed.op_index];
            if let AsmOp::Instruction(inst) = &op.op
                && let Some((rs1, rs2, _, funct3)) = conditional_branch_parts(inst)
            {
                let pc = layout.text_user_base.checked_add(placed.offset as u64).ok_or_else(layout_overflow)?;
                let target = branch_target(inst, self, layout).map_err(|error| op.provenance.attach(error))?;
                let result = if self.relaxed_text_branches.contains(&placed.op_index) {
                    encode_j_type(0x6f, 0, relative_offset(pc + 4, target)?)
                } else {
                    encode_b_type(0x63, funct3, rs1, rs2, relative_offset(pc, target)?)
                };
                result.map_err(|error| op.provenance.attach(error))?;
            }
        }
        Ok(())
    }
}

fn place_ops(
    ops: &[LocatedAsmOp],
    section: SectionKind,
    mode: BranchSizeMode<'_>,
    symbols: &mut HashMap<String, SymbolDef>,
) -> Result<(usize, Vec<TextOpLayout>)> {
    let mut offset = 0;
    let mut placements = Vec::with_capacity(ops.len());
    for (op_index, op) in ops.iter().enumerate() {
        if let AsmOp::Label(label) = &op.op {
            symbols
                .get_mut(label)
                .ok_or_else(|| op.provenance.attach(CompileError::without_span("assembly label lost its definition")))?
                .offset = offset;
        }
        let size = if let Some(plan) = &op.immediate_plan {
            plan.len().checked_mul(4).ok_or_else(layout_overflow)
        } else {
            op_size(&op.op, offset, section, op_index, mode)
        }
        .map_err(|error| op.provenance.attach(error))?;
        if section == SectionKind::Text {
            if matches!(op.op, AsmOp::Instruction(_)) && offset % 4 != 0 {
                return Err(op.provenance.attach(CompileError::without_span("executable instruction is not four-byte aligned")));
            }
            if matches!(op.op, AsmOp::Align(_)) && size % 4 != 0 {
                return Err(op
                    .provenance
                    .attach(CompileError::without_span("executable alignment requires whole four-byte NOP padding")));
            }
        }
        placements.push(TextOpLayout { op_index, offset, size });
        offset = checked_layout_add(offset, size).map_err(|error| op.provenance.attach(error))?;
    }
    Ok((offset, placements))
}

impl MachineLayoutPlan {
    pub(in crate::codegen) fn build(lines: &[String]) -> Result<Self> {
        Self::build_with_call_policy(lines, false)
    }

    pub(super) fn build_production(lines: &[String]) -> Result<Self> {
        Self::build_with_call_policy(lines, true)
    }

    fn build_with_call_policy(lines: &[String], production: bool) -> Result<Self> {
        let mut parsed = ParsedAssembly::parse(lines)?;
        parsed.validate_calls(production)?;
        let conservative = parsed.place(BranchSizeMode::Conservative)?;
        let mut long = parsed.required_long_branches(&conservative)?;
        let layout = loop {
            let layout = parsed.place(BranchSizeMode::Exact(&long))?;
            let required = parsed.required_long_branches(&layout)?;
            let previous = long.len();
            long.extend(required);
            if previous == long.len() {
                break layout;
            }
        };
        // Every additional pass promotes at least one of finitely many
        // branches. Already-long decisions are never removed, even if another
        // promotion changes alignment and would make shortening possible.
        parsed.relaxed_text_branches = long;
        parsed.validate_final_branches(&layout)?;
        let cfg = machine_cfg(&parsed)?;
        let coverage = validate_machine_block_coverage(&parsed, &cfg)?;
        let order = machine_layout_order(&cfg)?;
        let metrics = parsed.layout_metrics(&layout, &cfg, &order, coverage)?;
        Ok(Self { parsed, layout, cfg, order, metrics })
    }
}
