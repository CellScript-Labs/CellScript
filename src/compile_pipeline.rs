//! Structural checks, source validation, optional optimization and artifact construction.
use super::*;

pub(super) fn prepare_compile_ir(
    ast: &ast::Module,
    options: &CompileOptions,
    resolver: Option<(&ModuleResolver, &str)>,
    entry_scope: Option<&CompileEntryScope>,
) -> Result<(Option<ast::Module>, ir::IrModule)> {
    // 2.5. Primitive compat/strict mode: reject v0.14 protocol verbs in strict mode
    if options.is_primitive_strict_015() {
        check_primitive_strict_015(ast)?;
    };

    // 3. Type check
    if let Some((resolver, module_name)) = resolver {
        types::check_with_resolver(ast, resolver, module_name)?;
    } else {
        types::check(ast)?;
    }
    flow::check(ast)?;

    // Validate source lowering before optional rewrites can erase declarations.
    // Retired syntax and invalid helper bodies must not become acceptable
    // merely because inlining or dead-code elimination removes their evidence.
    let generate_ir = |module: &ast::Module| match resolver {
        Some((resolver, module_name)) => ir::generate_with_resolver(module, resolver, module_name),
        None => ir::generate(module),
    };
    let source_ir = generate_ir(ast)?;

    let optimized_ast = if options.diagnostics.enabled(OptimizationPass::ConstantFolding)
        || options.diagnostics.enabled(OptimizationPass::Inlining)
        || options.diagnostics.enabled(OptimizationPass::DeadCode)
    {
        let mut optimized = ast.clone();
        optimize::optimize_module(&mut optimized, options.diagnostics)?;
        optimize::preserve_effect_contracts(ast, &mut optimized, resolver.map(|(resolver, _)| resolver))?;
        if let Some((resolver, module_name)) = resolver {
            types::check_with_resolver(&optimized, resolver, module_name)?;
        } else {
            types::check(&optimized)?;
        }
        flow::check(&optimized)?;
        Some(optimized)
    } else {
        None
    };
    let lowering_ast = optimized_ast.as_ref().unwrap_or(ast);

    // 4. Generate IR
    let ir = if optimized_ast.is_some() { generate_ir(lowering_ast)? } else { source_ir };
    let mut ir = match entry_scope {
        Some(scope) => scope_ir_to_entry(&ir, scope)?,
        None => ir,
    };
    // Artifact selection resolves physical source/group bindings. Reconstruct
    // those facts before representation-only IR optimization.
    let scoped_facts = effects::populate(&mut ir)?;
    if options.diagnostics.enabled(OptimizationPass::ExactCellEquality) {
        ir::optimize_exact_cell_data_equality(&mut ir);
    }
    if options.diagnostics.enabled(OptimizationPass::SourceByteEquality) {
        ir::optimize_source_byte_equality(&mut ir);
    }
    let optimized_facts = effects::infer_ir(&ir)?;
    for item in &ir.items {
        let name = match item {
            ir::IrItem::Action(value) => &value.name,
            ir::IrItem::Function(value) => &value.name,
            ir::IrItem::Lock(value) => &value.name,
            _ => continue,
        };
        if scoped_facts.get(name) != optimized_facts.get(name) {
            return Err(error::SourceDiagnosticCode::UnresolvedEffect
                .error(format!("IR optimization changed the resolved effect contract of '{name}'"), error::Span::default()));
        }
    }
    executable_surface::validate_ir_module(&ir)?;
    Ok((optimized_ast, ir))
}

pub(super) fn compile_ast_with_build(
    ast: &ast::Module,
    options: &CompileOptions,
    resolver: Option<(&ModuleResolver, &str)>,
    build: Option<&BuildConfig>,
    ckb_deploy: Option<&package::CkbDeployConfig>,
    entry_scope: Option<&CompileEntryScope>,
    executable_surface_policy: ExecutableSurfacePolicy,
) -> Result<CompileResult> {
    validate_compile_options(options)?;
    let target_profile = TargetProfile::from_options(options, build)?;
    target_profile.ensure_compile_supported()?;
    let artifact_format = ArtifactFormat::from_target(resolve_target(options, build))?;
    let (optimized_ast, ir) = prepare_compile_ir(ast, options, resolver, entry_scope)?;
    let lowering_ast = optimized_ast.as_ref().unwrap_or(ast);
    let ir = &ir;

    let mut metadata =
        compile_metadata_from_ir(ir, artifact_format, target_profile, options.edition, options.primitive_compat.as_deref());
    bind_public_interface(&mut metadata, ast, resolver.map(|(resolver, _)| resolver))?;
    apply_trusted_external_verifiers(&mut metadata, ir, ckb_deploy.map_or(&[], |ckb| ckb.trusted_external_verifiers.as_slice()))?;
    if let Some(ckb) = ckb_deploy {
        zk_package::validate(ir, ckb)?;
    }
    bind_typed_semantics(&mut metadata, ir, ast, resolver.map(|(resolver, _)| resolver), options.source_contracts);
    let target_policy_violations = target_profile_artifact_policy_violations(&metadata, target_profile);
    if !target_policy_violations.is_empty() {
        return Err(CompileError::without_span(format!(
            "target profile policy failed for '{}':\n  - {}",
            target_profile.name(),
            target_policy_violations.join("\n  - ")
        )));
    }
    validate_executable_surface(&metadata, executable_surface_policy)?;

    // 5. Code generation
    let codegen_options = codegen::CodegenOptions { diagnostics: options.diagnostics, debug: options.debug, target_profile };
    let generated = codegen::generate_with_evidence(ir, &codegen_options, artifact_format).map_err(|error| {
        if error.code.is_some() {
            error
        } else {
            error.with_code("E2000")
        }
    })?;
    let mut artifact_bytes = generated.bytes;
    if artifact_bytes.is_empty() {
        return Err(CompileError::new("backend produced an empty artifact", error::Span::default()).with_code("E2001"));
    }

    // 5b. Debug info generation (embed DWARF section when debug option enabled and artifact is ELF).
    // Gated out of the wasm build: the debug module pulls native tooling
    // and DWARF embedding is not needed in the browser playground.
    #[cfg(not(feature = "wasm"))]
    if options.debug && artifact_format == ArtifactFormat::RiscvElf {
        let mut debug_gen =
            debug::DebugInfoGenerator::new(lowering_ast.name.clone(), std::path::PathBuf::from(lowering_ast.name.clone()));
        for item in &lowering_ast.items {
            if let ast::Item::Action(action) = item {
                for stmt in &action.body {
                    debug_gen.add_line_info(0, stmt_span(stmt));
                }
            }
        }
        let dwarf = debug_gen.generate_dwarf();
        let mut debug_sections = Vec::new();
        let mut elf_with_debug = artifact_bytes.clone();
        dwarf.write_to_elf(&mut elf_with_debug, &mut debug_sections);
        if !elf_with_debug.is_empty() {
            artifact_bytes = elf_with_debug;
        }
        metadata.debug_info_sections = debug_sections.iter().map(|s| s.name.clone()).collect();
    }

    if metadata.runtime.vm_abi.embedded_in_artifact {
        artifact_bytes = append_vm_abi_trailer(artifact_bytes, metadata.runtime.vm_abi.version);
    }
    let artifact_hash = ckb_blake2b256(&artifact_bytes);
    bind_artifact_metadata(&mut metadata, &artifact_bytes, &artifact_hash);
    bind_constraints_metadata(&mut metadata, &artifact_bytes, artifact_format, target_profile, ir, &codegen_options)?;
    metadata.runtime.builder_assumptions = crate::assumptions::builder_assumptions_from_metadata(&metadata);
    metadata.runtime.proof_plan_soundness = crate::proof_plan::soundness::check_metadata(&metadata, options.is_assurance_strict_016());
    if options.is_assurance_strict_016() {
        crate::proof_plan::soundness::validate_metadata(&metadata, true)?;
    }
    if options.is_ckb_source_strict_017() {
        validate_primitive_strict_017_metadata(&metadata)?;
    }

    let verified_artifact_draft =
        generated.machine_layout.map(|layout| verified_artifact::VerifiedArtifactDraft::new(layout, lowering_ast, ir));
    Ok(CompileResult {
        artifact_bytes,
        artifact_format,
        artifact_hash,
        metadata,
        ast: ast.clone(),
        verified_lowering_record: None,
        source_artifact_map: None,
        verified_artifact_draft,
        cache_hit: false,
    })
}
