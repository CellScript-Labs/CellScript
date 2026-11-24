use cellscript::artifact::{compile_artifact, ArtifactDeclaration};
use cellscript::{CompileOptions, CompileResult, ExecutableSurfacePolicy};

/// Supply the artifact declaration for a one-action source fixture. Tests
/// of context resolution itself use explicit declarations or bare compilation.
pub fn compile_fixture(source: &str, options: CompileOptions) -> cellscript::error::Result<CompileResult> {
    let module = cellscript::frontend::parse(source, options.edition)?;
    for item in &module.items {
        if let cellscript::ast::Item::Action(action) = item
            && action.next_surface.is_some()
            && let Some(cellscript::ast::Type::Named(schema)) = action.outputs.first().map(|output| &output.ty).or_else(|| {
                action.params.iter().find(|param| param.source == cellscript::ast::ParamSource::Input).map(|param| &param.ty)
            })
        {
            let declaration = ArtifactDeclaration::single_type_group("fixture", &action.name, schema);
            return compile_artifact(source, options, declaration, ExecutableSurfacePolicy::AllowFailClosed);
        }
    }
    cellscript::compile(source, options)
}
