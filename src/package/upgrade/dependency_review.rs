//! Review dependency-owned relations using the exact graph admitted by each
//! consuming root, including actions the root does not import or call.

use super::{compile_result_evidence, coordinate, display_coordinate, locked_source_identity, sha256, UpgradeCompileEvidence};
use crate::error::{CompileError, Result};
use crate::package::{LockedSource, Lockfile, PackageManager, ResolutionOptions, ResolvedPackage};
use crate::{CompileOptions, CompileResult, LoadedModule};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::path::{Component, Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(deny_unknown_fields)]
pub struct UpgradeDependencyReviewOwner {
    pub member: String,
    pub environment: Option<String>,
    pub old_node_id: Option<String>,
    pub new_node_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct UpgradeDependencySchemaImpact {
    pub id: String,
    pub coordinate: String,
    pub old_graph_hash: String,
    pub new_graph_hash: String,
    pub old: Option<UpgradeCompileEvidence>,
    pub new: Option<UpgradeCompileEvidence>,
    pub owners: Vec<UpgradeDependencyReviewOwner>,
}

#[derive(Default)]
pub(super) struct DependencyReviewState {
    compilations: BTreeMap<String, BTreeMap<String, CompileResult>>,
    impacts: BTreeMap<String, UpgradeDependencySchemaImpact>,
    reviews: BTreeMap<String, Vec<super::UpgradeSchemaReview>>,
}

struct LockedView<'a> {
    authority_root: &'a Path,
    lock: &'a Lockfile,
    packages: BTreeMap<String, ResolvedPackage>,
}

impl<'a> LockedView<'a> {
    fn load(authority_root: &'a Path, source_root: &Path, lock: &'a Lockfile, options: &ResolutionOptions) -> Result<Self> {
        let mut options = options.clone();
        options.offline = true;
        let packages = crate::package::with_lockfile_override(source_root, lock.clone(), || {
            let mut manager = PackageManager::new(source_root);
            manager.resolve_locked_dependencies(&options)?;
            Ok(manager.get_resolved().clone())
        })?;
        Ok(Self { authority_root, lock, packages })
    }

    fn by_coordinate(&self) -> BTreeMap<String, &ResolvedPackage> {
        self.packages
            .values()
            .map(|package| {
                let coordinate = package
                    .namespace
                    .as_deref()
                    .map_or_else(|| package.name.clone(), |namespace| format!("{namespace}/{}", package.name));
                (coordinate, package)
            })
            .collect()
    }

    fn closure(&self, root: &ResolvedPackage) -> Result<BTreeSet<String>> {
        let mut pending = VecDeque::from([root.node_id.clone()]);
        let mut nodes = BTreeSet::new();
        while let Some(node) = pending.pop_front() {
            if !nodes.insert(node.clone()) {
                continue;
            }
            let package = self
                .packages
                .get(&node)
                .ok_or_else(|| CompileError::without_span("locked dependency review closure contains a missing node"))?;
            pending.extend(package.dependencies.values().cloned());
        }
        Ok(nodes)
    }

    fn graph_hash(&self, root: &ResolvedPackage) -> Result<String> {
        let mut nodes = Vec::new();
        for id in self.closure(root)? {
            let locked = &self.lock.dependencies[&id];
            let (kind, source) = match &locked.source {
                LockedSource::Path { path } => ("path".to_string(), normalized_absolute(&self.authority_root.join(path))?),
                source => locked_source_identity(source),
            };
            let selection = id.split_once("|compiler=").map(|(_, selection)| selection).ok_or_else(|| {
                CompileError::without_span("locked dependency has no compiler/feature/environment selection identity")
            })?;
            let edges = locked
                .dependencies
                .iter()
                .map(|(alias, target)| (alias.clone(), display_coordinate(&coordinate(&self.lock.dependencies[target]))))
                .collect::<BTreeMap<_, _>>();
            nodes.push(serde_json::json!({
                "coordinate": display_coordinate(&coordinate(locked)), "version": locked.version,
                "source_kind": kind, "source_identity": source, "source_hash": locked.source_hash,
                "manifest_digest": locked.manifest_digest, "compiler_requirement": locked.compiler_requirement,
                "selection": selection, "dependencies": edges,
            }));
        }
        nodes.sort_by_key(|value| value["coordinate"].as_str().unwrap_or_default().to_string());
        Ok(sha256(&serde_json::to_vec(&serde_json::json!({
            "domain": "cellscript-upgrade-dependency-source-graph-v2", "root": root.name, "nodes": nodes,
        }))?))
    }

    fn compile(&self, root: &ResolvedPackage) -> Result<BTreeMap<String, CompileResult>> {
        let mut modules = BTreeMap::<PathBuf, LoadedModule>::new();
        let mut entries = BTreeSet::new();
        for id in self.closure(root)? {
            let path = &self.packages[&id].path;
            let input = camino::Utf8Path::from_path(path)
                .ok_or_else(|| CompileError::without_span("dependency source path is not valid UTF-8"))?;
            // Use the same complete source set as source hashing. A dependency
            // may have only lib.cell, several independent modules, or no app
            // entry at all. Discovery neither consults its independent lock
            // nor selects dependencies.
            for source_path in crate::collect_package_cell_files(input)? {
                if id == root.node_id {
                    entries.insert(source_path.clone());
                }
                let edition = crate::source_edition(&source_path)?;
                let source = crate::read_module_source(&source_path)?;
                let module = crate::parse_loaded_module(source_path, source, edition)?;
                modules.insert(module.path.clone().into_std_path_buf(), module);
            }
        }
        let modules = modules.into_values().collect::<Vec<_>>();
        let mut compiled = BTreeMap::new();
        for entry in entries {
            let result = crate::compile_pipeline::compile_loaded_package_modules(
                &root.path,
                &entry,
                modules.clone(),
                CompileOptions::default(),
            )?;
            compiled.insert(result.metadata.module.clone(), result);
        }
        Ok(compiled)
    }
}

impl DependencyReviewState {
    pub(super) fn collect(
        &mut self,
        authority_root: &Path,
        old_source_root: &Path,
        old_lock: Option<&Lockfile>,
        new_lock: Option<&Lockfile>,
        options: &ResolutionOptions,
        member: &str,
        acknowledgements: &[super::UpgradeSchemaAcknowledgement],
    ) -> Result<()> {
        let old = old_lock.map(|lock| LockedView::load(authority_root, old_source_root, lock, options)).transpose()?;
        let new = new_lock.map(|lock| LockedView::load(authority_root, authority_root, lock, options)).transpose()?;
        let old_coordinates = old.as_ref().map(LockedView::by_coordinate).unwrap_or_default();
        let new_coordinates = new.as_ref().map(LockedView::by_coordinate).unwrap_or_default();
        for coordinate in old_coordinates.keys().chain(new_coordinates.keys()).collect::<BTreeSet<_>>() {
            let old_package = old_coordinates.get(coordinate).copied();
            let new_package = new_coordinates.get(coordinate).copied();
            let old_graph_hash = old_package
                .map(|package| old.as_ref().unwrap().graph_hash(package))
                .transpose()?
                .unwrap_or_else(|| "absent".to_string());
            let new_graph_hash = new_package
                .map(|package| new.as_ref().unwrap().graph_hash(package))
                .transpose()?
                .unwrap_or_else(|| "absent".to_string());
            if old_graph_hash == new_graph_hash {
                continue;
            }
            if let Some(package) = old_package
                && !self.compilations.contains_key(&old_graph_hash)
            {
                self.compilations.insert(old_graph_hash.clone(), old.as_ref().unwrap().compile(package)?);
            }
            if let Some(package) = new_package
                && !self.compilations.contains_key(&new_graph_hash)
            {
                self.compilations.insert(new_graph_hash.clone(), new.as_ref().unwrap().compile(package)?);
            }
            let old_compilations = self.compilations.get(&old_graph_hash);
            let new_compilations = self.compilations.get(&new_graph_hash);
            let modules = old_compilations
                .into_iter()
                .flat_map(BTreeMap::keys)
                .chain(new_compilations.into_iter().flat_map(BTreeMap::keys))
                .collect::<BTreeSet<_>>();
            for module in modules {
                let identity = sha256(&serde_json::to_vec(&(coordinate, module, &old_graph_hash, &new_graph_hash))?);
                let id = format!("dependency:{}", identity.trim_start_matches("sha256:"));
                if !self.impacts.contains_key(&id) {
                    let old_compiled = old_compilations.and_then(|compilations| compilations.get(module));
                    let new_compiled = new_compilations.and_then(|compilations| compilations.get(module));
                    let reviews = super::schema_review::reviews(
                        &id,
                        &None,
                        old_compiled,
                        new_compiled,
                        &old_graph_hash,
                        &new_graph_hash,
                        acknowledgements,
                    )?;
                    self.reviews.insert(id.clone(), reviews);
                    self.impacts.insert(
                        id.clone(),
                        UpgradeDependencySchemaImpact {
                            id: id.clone(),
                            coordinate: coordinate.clone(),
                            old_graph_hash: old_graph_hash.clone(),
                            new_graph_hash: new_graph_hash.clone(),
                            old: old_compiled.map(|result| compile_result_evidence(Some(result))),
                            new: new_compiled.map(|result| compile_result_evidence(Some(result))),
                            owners: Vec::new(),
                        },
                    );
                }
                self.impacts.get_mut(&id).unwrap().owners.push(UpgradeDependencyReviewOwner {
                    member: member.to_string(),
                    environment: options.environment.clone(),
                    old_node_id: old_package.map(|package| package.node_id.clone()),
                    new_node_id: new_package.map(|package| package.node_id.clone()),
                });
            }
        }
        Ok(())
    }

    pub(super) fn finish(mut self) -> (Vec<UpgradeDependencySchemaImpact>, Vec<super::UpgradeSchemaReview>) {
        for impact in self.impacts.values_mut() {
            impact.owners.sort();
            impact.owners.dedup();
        }
        (self.impacts.into_values().collect(), self.reviews.into_values().flatten().collect())
    }
}

fn normalized_absolute(path: &Path) -> Result<String> {
    if !path.is_absolute() {
        return Err(CompileError::without_span("dependency authority root must be absolute"));
    }
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::ParentDir => {
                normalized.pop();
            }
            Component::CurDir => {}
            other => normalized.push(other.as_os_str()),
        }
    }
    Ok(normalized.to_string_lossy().replace('\\', "/"))
}
