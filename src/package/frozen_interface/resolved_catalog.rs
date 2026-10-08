//! Native resolver ownership for the defining baseline source snapshot.
//! No raw owner label, exported context or same-width nominal can mint this
//! value. Source-level generic handles and policy authorization remain separate.
use super::{checker_error, invalid, FrozenCodeCatalog, FrozenPackageModule, Package, Source};
use crate::error::Result;
use cellscript_artifact_checker::{canonical_bytes, canonical_hash};
use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Debug)]
pub struct ResolvedCodeCatalog {
    consumer: FrozenPackageModule,
    catalog: FrozenCodeCatalog,
    owner: Owner,
    owner_id: String,
    identity: String,
}
#[derive(Debug, Serialize)]
struct Owner {
    schema: &'static str,
    defining_package: String,
    defining_module: String,
    required_module_contract: String,
    source_closure_packages: Vec<String>,
}
#[derive(Serialize)]
struct Binding<'a> {
    schema: &'static str,
    consumer_source_context: &'a str,
    code_catalog: &'a str,
    defining_owner: &'a str,
}
impl ResolvedCodeCatalog {
    pub fn consumer(&self) -> &FrozenPackageModule {
        &self.consumer
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
    fn binding(&self) -> Binding<'_> {
        Binding {
            schema: "cellscript-resolver-code-catalog-v1",
            consumer_source_context: self.consumer.context_identity(),
            code_catalog: self.catalog.identity(),
            defining_owner: &self.owner_id,
        }
    }
}
fn same_snapshot(left: &Package, right: &Package) -> bool {
    left.name == right.name
        && left.namespace == right.namespace
        && left.version == right.version
        && left.edition == right.edition
        && left.manifest_digest == right.manifest_digest
        && left.source_hash == right.source_hash
        && left.compiler_requirement == right.compiler_requirement
}
/// Rebind a separately root-compiled baseline to its actual defining owner in
/// the consumer's pinned source closure. Root compilation must not overwrite a
/// selected Git/Registry/Local origin with a RootSnapshot label. Dependency
/// aliases resolve to the same defining package identity; caller context bytes
/// remain outside that owner identity. A different owner or changed source,
/// manifest, selected transitive origin, module path or network rejects.
/// The existing proofs are consumed intact. This is source ownership evidence,
/// not full I syntax, H1 admission, an approved root or on-chain enforcement.
pub fn resolve_code_catalog_source(consumer: FrozenPackageModule, catalog: FrozenCodeCatalog) -> Result<ResolvedCodeCatalog> {
    let mut total = 0usize;
    let mut preflight = |bytes: &[u8]| -> Result<()> {
        total = total.checked_add(bytes.len()).ok_or_else(|| invalid("resolved code catalog byte overflow"))?;
        if bytes.len() > 4 * 1024 * 1024 || total > 16 * 1024 * 1024 {
            return Err(invalid("resolved code catalog exceeds 4 MiB/file or shared 16 MiB"));
        }
        Ok(())
    };
    for module in std::iter::once(&consumer)
        .chain(std::iter::once(catalog.required()))
        .chain(catalog.candidates().iter().map(|candidate| candidate.module()))
    {
        for bytes in module.bundle() {
            preflight(bytes)?;
        }
    }
    for candidate in catalog.candidates() {
        preflight(candidate.raw_transaction())?;
        preflight(candidate.selected_script())?;
    }
    let required = catalog.required();
    if consumer.context.chain_id != required.context.chain_id || consumer.context.network_genesis != required.context.network_genesis {
        return Err(invalid("resolved code catalog has conflicting pinned chain identities"));
    }
    let module = &required.context.entry_module;
    let baseline = required.context.modules.get(module).ok_or_else(|| invalid("defining baseline module is absent"))?;
    let selected =
        consumer.context.modules.get(module).ok_or_else(|| invalid("consumer did not resolve the defining baseline module"))?;
    let baseline_package = &required.context.packages[&baseline.package];
    let selected_package = &consumer.context.packages[&selected.package];
    if !matches!(baseline_package.source, Source::RootSnapshot) || !same_snapshot(baseline_package, selected_package) {
        return Err(invalid("resolved defining package differs from the root-compiled baseline snapshot"));
    }
    let mut owners = BTreeMap::from([(baseline.package.clone(), selected.package.clone())]);
    for (identity, package) in &required.context.packages {
        if identity == &baseline.package {
            continue;
        }
        let selected = consumer
            .context
            .packages
            .get(identity)
            .ok_or_else(|| invalid("consumer selected another transitive source origin or snapshot"))?;
        if !same_snapshot(package, selected) {
            return Err(invalid("consumer selected another transitive source snapshot"));
        }
        // Package identities already hash the complete actual source record;
        // dependency origin is never erased by comparing only source contents.
        owners.insert(identity.clone(), identity.clone());
    }
    for (name, baseline_module) in &required.context.modules {
        let selected_module = consumer.context.modules.get(name).ok_or_else(|| invalid("consumer lacks a baseline source module"))?;
        if owners.get(&baseline_module.package) != Some(&selected_module.package)
            || baseline_module.relative_path != selected_module.relative_path
            || baseline_module.source_hash != selected_module.source_hash
            || baseline_module.source_bytes != selected_module.source_bytes
        {
            return Err(invalid("resolved defining module ownership/path/bytes differ from baseline"));
        }
    }
    // No extra module may silently enter the same defining package snapshot.
    for (name, selected_module) in &consumer.context.modules {
        if selected_module.package == selected.package && !required.context.modules.contains_key(name) {
            return Err(invalid("consumer defining snapshot has an additional baseline module"));
        }
    }
    let owner = Owner {
        schema: "cellscript-resolver-interface-source-owner-v1",
        defining_package: selected.package.clone(),
        defining_module: module.clone(),
        required_module_contract: required.projection().identity().into(),
        source_closure_packages: owners.values().cloned().collect::<std::collections::BTreeSet<_>>().into_iter().collect(),
    };
    let owner_id = canonical_hash("cellscript-resolver-interface-source-owner-id-v1", &owner).map_err(checker_error)?;
    let mut checked = ResolvedCodeCatalog { consumer, catalog, owner, owner_id, identity: String::new() };
    checked.identity = canonical_hash("cellscript-resolver-code-catalog-id-v1", &checked.binding()).map_err(checker_error)?;
    Ok(checked)
}
