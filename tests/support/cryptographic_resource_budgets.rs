#![allow(dead_code)]

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResourceMetrics {
    pub cycles: u64,
    pub elf_bytes: usize,
    pub max_stack_frame_bytes: u32,
    pub witness_bytes: usize,
    pub transaction_bytes: usize,
    pub dependency_bytes: usize,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResourceBudgetProfile {
    pub id: String,
    pub status: String,
    pub capabilities: Vec<String>,
    pub maximum_shape: Vec<String>,
    pub evidence: Vec<String>,
    pub measurement_policy: String,
    pub measured: Option<ResourceMetrics>,
    pub budgets: ResourceMetrics,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CryptographicResourceBudgets {
    pub schema: String,
    pub measurement_backend: String,
    pub profiles: Vec<ResourceBudgetProfile>,
}

pub fn manifest() -> CryptographicResourceBudgets {
    serde_json::from_str(include_str!("../fixtures/cryptographic_resource_budgets.json"))
        .expect("checked-in cryptographic resource budget manifest")
}

pub fn profile(id: &str) -> ResourceBudgetProfile {
    manifest().profiles.into_iter().find(|profile| profile.id == id).unwrap_or_else(|| panic!("missing resource profile {id}"))
}

pub fn assert_metrics(id: &str, actual: &ResourceMetrics) {
    let profile = profile(id);
    assert_eq!(profile.status, "passed", "resource profile {id} must be executable in the ordinary test gate");
    let measured = profile.measured.unwrap_or_else(|| panic!("resource profile {id} has no recorded measurements"));
    match profile.measurement_policy.as_str() {
        "exact" => assert_eq!(&measured, actual, "recorded cryptographic resource measurement is stale for {id}"),
        "bounded-signature-dependent-cycles" => {
            assert!(actual.cycles > 0, "{id} must consume cycles");
            assert_eq!(actual.elf_bytes, measured.elf_bytes, "recorded {id} ELF measurement is stale");
            assert_eq!(actual.max_stack_frame_bytes, measured.max_stack_frame_bytes, "recorded {id} stack measurement is stale");
            assert_eq!(actual.witness_bytes, measured.witness_bytes, "recorded {id} witness measurement is stale");
            assert_eq!(actual.transaction_bytes, measured.transaction_bytes, "recorded {id} transaction measurement is stale");
            assert_eq!(actual.dependency_bytes, measured.dependency_bytes, "recorded {id} dependency measurement is stale");
        }
        policy => panic!("unknown resource measurement policy {policy} for {id}"),
    }
    assert!(actual.cycles <= profile.budgets.cycles, "{id} cycles exceeded its budget");
    assert!(actual.elf_bytes <= profile.budgets.elf_bytes, "{id} ELF bytes exceeded its budget");
    assert!(actual.max_stack_frame_bytes <= profile.budgets.max_stack_frame_bytes, "{id} stack frame exceeded its budget");
    assert!(actual.witness_bytes <= profile.budgets.witness_bytes, "{id} witness bytes exceeded its budget");
    assert!(actual.transaction_bytes <= profile.budgets.transaction_bytes, "{id} transaction bytes exceeded its budget");
    assert!(actual.dependency_bytes <= profile.budgets.dependency_bytes, "{id} dependency bytes exceeded its budget");
}
