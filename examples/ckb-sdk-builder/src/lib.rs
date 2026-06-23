//! Cookbook wrapper for the formal CellScript CKB adapter crate.
//!
//! The reusable implementation lives in `crates/cellscript-ckb-adapter`.
//! This example crate stays intentionally small so the checked-in cookbook does
//! not become a second adapter implementation.

pub use cellscript_ckb_adapter::*;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cookbook_uses_formal_adapter_crate() {
        let resolved = sample_resolved_action_tx();
        let (_tx, evidence) = build_action_transaction(&resolved).unwrap();

        assert_eq!(evidence.schema, ACTION_ACCEPTANCE_REPORT_SCHEMA);
        assert_eq!(evidence.state, "ResolvedActionTx");
        assert!(!evidence.ckb_vm_execution);
        assert!(!evidence.tx_pool_acceptance);
    }
}
