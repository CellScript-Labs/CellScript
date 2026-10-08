//! Parsed, lock-pinned source closure before consumer type checking.
//! A snapshot does not certify a consumer ELF or make an unknown type usable.
use super::super::{DependencyScope, ResolutionOptions};
use super::resolved_catalog::{checked_source_owner, Owner};
use super::{capture, checker_error, invalid, Context, FrozenCodeCatalog};
use crate::error::Result;
use camino::{Utf8Path, Utf8PathBuf};
use cellscript_artifact_checker::{canonical_bytes, canonical_hash};
use std::collections::BTreeMap;

#[derive(Debug)]
pub struct FrozenPackageSources {
    pub(super) context: Context,
    identity: String,
    root: Utf8PathBuf,
    environment: String,
    paths: BTreeMap<Utf8PathBuf, String>,
}
impl FrozenPackageSources {
    pub fn context_identity(&self) -> &str {
        &self.identity
    }
    pub fn context_bytes(&self) -> Result<Vec<u8>> {
        canonical_bytes(&self.context).map_err(checker_error)
    }
    /// Re-read the same actual pinned root without repinning or network access.
    /// File paths are private locators, never defining-owner identities.
    pub fn check_unchanged(&self) -> Result<()> {
        if super::super::active_lockfile_override(self.root.as_std_path())?.is_some() {
            return Err(invalid("frozen source closure cannot use a planned lockfile override"));
        }
        let resolution = options(&self.environment);
        super::super::with_resolution_options(resolution.clone(), || {
            let (context, paths) = capture(&self.root, &self.environment, &resolution)?;
            if context != self.context || paths != self.paths {
                return Err(invalid("frozen source closure changed; capture and bind the actual pinned sources again"));
            }
            Ok(())
        })
    }
}
fn options(environment: &str) -> ResolutionOptions {
    ResolutionOptions { scope: DependencyScope::Runtime, environment: Some(environment.into()), offline: true, ..Default::default() }
}
/// Capture the actual bounded/parsed source and lock closure before resolving
/// new consumer types. No compiled consumer is needed to obtain its source
/// owners. Unknown semantic types may still be rejected by subsequent typing.
/// The private snapshot stores hashes/context, not all source file contents.
/// It does not prove body semantics, checker acceptance or deployment authority.
pub fn freeze_package_sources(root: &Utf8Path, environment: &str) -> Result<FrozenPackageSources> {
    let root = crate::canonical_utf8_path(root)?;
    if !root.is_dir() {
        return Err(invalid("frozen source closure requires a package directory"));
    }
    if super::super::active_lockfile_override(root.as_std_path())?.is_some() {
        return Err(invalid("frozen source closure cannot use a planned lockfile override"));
    }
    let resolution = options(environment);
    super::super::with_resolution_options(resolution.clone(), || {
        let (context, paths) = capture(&root, environment, &resolution)?;
        let identity = canonical_hash("cellscript-frozen-module-source-context-id-v1", &context).map_err(checker_error)?;
        Ok(FrozenPackageSources { context, identity, root, environment: environment.into(), paths })
    })
}

#[derive(Debug)]
pub struct ResolvedSourceCatalog {
    sources: FrozenPackageSources,
    catalog: FrozenCodeCatalog,
    owner: Owner,
    owner_id: String,
    identity: String,
}
#[derive(serde::Serialize)]
struct Binding<'a> {
    schema: &'static str,
    consumer_source_context: &'a str,
    code_catalog: &'a str,
    defining_owner: &'a str,
}
impl ResolvedSourceCatalog {
    pub fn sources(&self) -> &FrozenPackageSources {
        &self.sources
    }
    pub fn catalog(&self) -> &FrozenCodeCatalog {
        &self.catalog
    }
    pub fn source_owner_identity(&self) -> &str {
        &self.owner_id
    }
    pub fn source_owner_bytes(&self) -> Result<Vec<u8>> {
        canonical_bytes(&self.owner).map_err(checker_error)
    }
    pub fn identity(&self) -> &str {
        &self.identity
    }
    pub fn canonical_bytes(&self) -> Result<Vec<u8>> {
        canonical_bytes(&self.binding()).map_err(checker_error)
    }
    pub fn check_unchanged_sources(&self) -> Result<()> {
        self.sources.check_unchanged()
    }
    fn binding(&self) -> Binding<'_> {
        Binding {
            schema: "cellscript-resolver-source-catalog-v1",
            consumer_source_context: self.sources.context_identity(),
            code_catalog: self.catalog.identity(),
            defining_owner: &self.owner_id,
        }
    }
}
/// Bind actual parsed sources before type checking; no consumer ELF or usable
/// source type is inferred. The actual checked catalog stays privately owned.
/// Source capture and catalog checking retain their separate 16 MiB bounds.
/// No parser is added here. Later compilation must verify the captured sources
/// remain unchanged before accepting a type-context binding.
pub fn resolve_source_catalog(sources: FrozenPackageSources, catalog: FrozenCodeCatalog) -> Result<ResolvedSourceCatalog> {
    sources.check_unchanged()?;
    let owner = checked_source_owner(&sources.context, &catalog)?;
    let owner_id = canonical_hash("cellscript-resolver-interface-source-owner-id-v1", &owner).map_err(checker_error)?;
    let mut checked = ResolvedSourceCatalog { sources, catalog, owner, owner_id, identity: String::new() };
    checked.identity = canonical_hash("cellscript-resolver-source-catalog-id-v1", &checked.binding()).map_err(checker_error)?;
    Ok(checked)
}
