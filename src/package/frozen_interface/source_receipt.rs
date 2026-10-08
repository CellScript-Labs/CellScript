//! Native source facts bound to an actual independent finite artifact receipt.
//! Source package coordinates are not authenticated publisher/Registry facts.
use super::{checker_error, invalid, FrozenPackageModule};
use crate::error::Result;
use cellscript_artifact_checker::fixed_policy_receipt::CheckedFixedPolicyReceipt;
use cellscript_artifact_checker::{canonical_bytes, canonical_hash};
use semver::Version;
use serde::Serialize;

#[derive(Debug)]
pub struct CheckedSourceCodeReceipt {
    record: Record,
    version: Version,
    identity: String,
}
#[derive(Debug, Serialize)]
struct Record {
    schema: &'static str,
    source_context: String,
    defining_module: String,
    package: SourcePackageVersion,
    artifact_receipt: String,
}
#[derive(Debug, Serialize)]
pub(super) struct SourcePackageVersion {
    pub(super) name: String,
    pub(super) namespace: Option<String>,
    pub(super) version: String,
    pub(super) edition: crate::CellScriptEdition,
}
impl CheckedSourceCodeReceipt {
    pub fn identity(&self) -> &str {
        &self.identity
    }
    pub fn canonical_bytes(&self) -> Result<Vec<u8>> {
        canonical_bytes(&self.record).map_err(checker_error)
    }
    pub fn source_context_identity(&self) -> &str {
        &self.record.source_context
    }
    pub fn defining_module(&self) -> &str {
        &self.record.defining_module
    }
    pub fn package_name(&self) -> &str {
        &self.record.package.name
    }
    pub fn package_namespace(&self) -> Option<&str> {
        self.record.package.namespace.as_deref()
    }
    pub fn package_version(&self) -> &Version {
        &self.version
    }
    pub fn edition(&self) -> &crate::CellScriptEdition {
        &self.record.package.edition
    }
    pub fn artifact_receipt_identity(&self) -> &str {
        &self.record.artifact_receipt
    }
    pub(super) fn package(&self) -> &SourcePackageVersion {
        &self.record.package
    }
}
pub(super) fn source_package_version(module: &FrozenPackageModule) -> Result<(SourcePackageVersion, Version)> {
    let owner = module
        .context
        .modules
        .get(&module.context.entry_module)
        .ok_or_else(|| invalid("source code receipt defining module is absent; capture actual source ownership"))?;
    let package = module
        .context
        .packages
        .get(&owner.package)
        .ok_or_else(|| invalid("source code receipt defining package is absent; capture actual source ownership"))?;
    // Apply finite text limits before version parsing, cloning or record hashing.
    if package.name.len() > 512
        || package.namespace.as_ref().is_some_and(|namespace| namespace.len() > 512)
        || package.version.len() > 128
        || module.context.entry_module.len() > 512
    {
        return Err(invalid("source code receipt exceeds coordinate/module text limits; use a bounded source package"));
    }
    let version = Version::parse(&package.version)
        .map_err(|_| invalid("source code receipt requires an actual SemVer package version; fix and repin the source manifest"))?;
    Ok((
        SourcePackageVersion {
            name: package.name.clone(),
            namespace: package.namespace.clone(),
            version: package.version.clone(),
            edition: package.edition,
        },
        version,
    ))
}
/// Only the native catalog factory calls this after checking this exact module
/// bundle and deployment inputs. Exported contexts/receipts cannot call it.
pub(super) fn bind_source_receipt(
    module: &FrozenPackageModule,
    receipt: &CheckedFixedPolicyReceipt,
) -> Result<CheckedSourceCodeReceipt> {
    let (package, version) = source_package_version(module)?;
    let record = Record {
        schema: "cellscript-frozen-source-code-receipt-v1",
        source_context: module.context_identity().into(),
        defining_module: module.context.entry_module.clone(),
        package,
        artifact_receipt: receipt.identity().into(),
    };
    let identity = canonical_hash("cellscript-frozen-source-code-receipt-id-v1", &record).map_err(checker_error)?;
    Ok(CheckedSourceCodeReceipt { record, version, identity })
}
