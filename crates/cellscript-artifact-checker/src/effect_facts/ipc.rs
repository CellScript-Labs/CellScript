//! VM2 IPC facts. Process observations and communication are separate from
//! transaction-context access; no category translation participates here.
use super::EffectFacts;
#[cfg(test)]
use super::ProcessOperation;
use crate::runtime_contract::IpcHelper;

impl IpcHelper {
    /// The complete IPC inventory requires a distinct explicit process fact.
    /// Availability and the ABI remain separate contracts: this does not make
    /// a deferred or failed spawn executable, nor confer lifecycle authority.
    pub fn effect_facts(self) -> Result<EffectFacts, super::runtime::RuntimeFactError> {
        self.helper().fact_contract().instantiate(&super::runtime::RuntimeArgumentFacts::default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    #[test]
    fn executable_ipc_has_distinct_facts() {
        let mut seen = BTreeSet::new();
        for helper in IpcHelper::ALL {
            let facts = helper.effect_facts().unwrap();
            assert!(facts.context_reads.is_empty(), "{}", helper.symbol());
            assert!(facts.lifecycle_ops.is_empty(), "{}", helper.symbol());
            assert_eq!(facts.process_ops.len(), 1, "{}", helper.symbol());
            assert!(!facts.is_empty());
            assert!(!facts.allows_observation());
            assert!(seen.insert(facts));
        }
        assert_eq!(seen.len(), 7);
        assert_eq!(IpcHelper::CkbPipeRead.effect_facts().unwrap(), EffectFacts::process(ProcessOperation::PipeRead));
        assert_eq!(IpcHelper::CkbPipeWrite.effect_facts().unwrap(), EffectFacts::process(ProcessOperation::PipeWrite));
        assert!(!IpcHelper::CkbPipeRead.effect_facts().unwrap().accepts(&IpcHelper::CkbPipeWrite.effect_facts().unwrap()));
        assert!(!IpcHelper::CkbProcessId.effect_facts().unwrap().is_empty());
    }
}
