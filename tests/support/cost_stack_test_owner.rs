include!("cost_stack.rs");

#[cfg(test)]
mod tests {
    use super::*;

    fn graph() -> StackAnalysis {
        StackAnalysis {
            // Caller frame (32), temporary outgoing arguments (16), callee
            // frame (48). The resulting bound must include all three.
            instructions: BTreeMap::from([
                (0, 0xfe010113),
                (4, 0xff010113),
                (8, 0x000000ef),
                (12, 0x01010113),
                (16, 0x02010113),
                (20, 0x00008067),
                (64, 0xfd010113),
                (68, 0x03010113),
                (72, 0x00008067),
            ]),
            adjustments: BTreeMap::from([(0, -32), (4, -16), (12, 16), (16, 32), (64, -48), (68, 48)]),
            targets: BTreeMap::from([(8, 64)]),
            active_calls: BTreeSet::new(),
            memo: BTreeMap::new(),
        }
    }

    #[test]
    fn temporary_outgoing_reservations_count_toward_nested_bound() {
        assert_eq!(graph().call_bound(0).unwrap(), 96);
    }

    #[test]
    fn recursion_and_growing_stack_loops_have_no_claimed_bound() {
        let mut recursive = graph();
        recursive.targets.insert(8, 0);
        assert!(recursive.call_bound(0).unwrap_err().contains("recursive"));
        let mut growing = graph();
        growing.instructions.insert(8, 0x0000006f);
        growing.targets.insert(8, 4);
        assert!(growing.call_bound(0).unwrap_err().contains("inconsistent stack"));
    }

    #[test]
    fn unknown_callees_and_unbalanced_returns_have_no_claimed_bound() {
        let mut missing = graph();
        missing.targets.insert(8, 128);
        assert!(missing.call_bound(0).unwrap_err().contains("unknown callee"));
        let mut unbalanced = graph();
        unbalanced.adjustments.remove(&68);
        assert!(unbalanced.call_bound(0).unwrap_err().contains("unbalanced"));
    }
}
