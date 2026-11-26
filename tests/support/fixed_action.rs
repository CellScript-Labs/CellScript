use cellscript::artifact::{compile_artifact, ArtifactDeclaration};
use cellscript::{CompileOptions, CompileResult, ExecutableSurfacePolicy};

#[path = "../../src/artifact/fixture_context.rs"]
mod fixture_context;

/// Supply the artifact declaration for a one-action source fixture. Tests
/// of context resolution itself use explicit declarations or bare compilation.
pub fn compile_fixture(source: &str, options: CompileOptions) -> cellscript::error::Result<CompileResult> {
    if let Some(context) = fixture_context::parse(source).map_err(cellscript::error::CompileError::without_span)? {
        let declaration = ArtifactDeclaration::single_type_group("fixture", context.entry, context.layout);
        return compile_artifact(source, options, declaration, ExecutableSurfacePolicy::AllowFailClosed);
    }
    cellscript::compile(source, options)
}
