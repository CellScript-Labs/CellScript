use crate::{ir::IrType, runtime_errors::CellScriptRuntimeError};

pub struct Collections;

impl Collections {
    pub fn functions() -> Vec<CollectionFunction> {
        vec![
            CollectionFunction { name: "vec_new".to_string(), params: vec![], return_type: Some(IrType::Named("Vec".to_string())) },
            CollectionFunction {
                name: "vec_with_capacity".to_string(),
                params: vec![("capacity".to_string(), IrType::U64)],
                return_type: Some(IrType::Named("Vec".to_string())),
            },
            CollectionFunction {
                name: "vec_push".to_string(),
                params: vec![
                    ("vec".to_string(), IrType::MutRef(Box::new(IrType::Named("Vec".to_string())))),
                    ("value".to_string(), IrType::U64),
                ],
                return_type: None,
            },
            CollectionFunction {
                name: "vec_pop".to_string(),
                params: vec![("vec".to_string(), IrType::MutRef(Box::new(IrType::Named("Vec".to_string()))))],
                return_type: Some(IrType::Named("Option".to_string())),
            },
            CollectionFunction {
                name: "vec_get".to_string(),
                params: vec![
                    ("vec".to_string(), IrType::Ref(Box::new(IrType::Named("Vec".to_string())))),
                    ("index".to_string(), IrType::U64),
                ],
                return_type: Some(IrType::Named("Option".to_string())),
            },
            CollectionFunction {
                name: "vec_len".to_string(),
                params: vec![("vec".to_string(), IrType::Ref(Box::new(IrType::Named("Vec".to_string()))))],
                return_type: Some(IrType::U64),
            },
            CollectionFunction {
                name: "vec_is_empty".to_string(),
                params: vec![("vec".to_string(), IrType::Ref(Box::new(IrType::Named("Vec".to_string()))))],
                return_type: Some(IrType::Bool),
            },
            CollectionFunction {
                name: "vec_clear".to_string(),
                params: vec![("vec".to_string(), IrType::MutRef(Box::new(IrType::Named("Vec".to_string()))))],
                return_type: None,
            },
            CollectionFunction {
                name: "vec_contains".to_string(),
                params: vec![
                    ("vec".to_string(), IrType::Ref(Box::new(IrType::Named("Vec".to_string())))),
                    ("value".to_string(), IrType::U64),
                ],
                return_type: Some(IrType::Bool),
            },
            CollectionFunction {
                name: "vec_remove".to_string(),
                params: vec![
                    ("vec".to_string(), IrType::MutRef(Box::new(IrType::Named("Vec".to_string())))),
                    ("index".to_string(), IrType::U64),
                ],
                return_type: Some(IrType::U64),
            },
            CollectionFunction {
                name: "vec_insert".to_string(),
                params: vec![
                    ("vec".to_string(), IrType::MutRef(Box::new(IrType::Named("Vec".to_string())))),
                    ("index".to_string(), IrType::U64),
                    ("value".to_string(), IrType::U64),
                ],
                return_type: None,
            },
            CollectionFunction {
                name: "vec_sort".to_string(),
                params: vec![("vec".to_string(), IrType::MutRef(Box::new(IrType::Named("Vec".to_string()))))],
                return_type: None,
            },
            CollectionFunction {
                name: "vec_reverse".to_string(),
                params: vec![("vec".to_string(), IrType::MutRef(Box::new(IrType::Named("Vec".to_string()))))],
                return_type: None,
            },
            CollectionFunction {
                name: "hashmap_new".to_string(),
                params: vec![],
                return_type: Some(IrType::Named("HashMap".to_string())),
            },
            CollectionFunction {
                name: "hashmap_with_capacity".to_string(),
                params: vec![("capacity".to_string(), IrType::U64)],
                return_type: Some(IrType::Named("HashMap".to_string())),
            },
            CollectionFunction {
                name: "hashmap_insert".to_string(),
                params: vec![
                    ("map".to_string(), IrType::MutRef(Box::new(IrType::Named("HashMap".to_string())))),
                    ("key".to_string(), IrType::U64),
                    ("value".to_string(), IrType::U64),
                ],
                return_type: Some(IrType::Named("Option".to_string())),
            },
            CollectionFunction {
                name: "hashmap_get".to_string(),
                params: vec![
                    ("map".to_string(), IrType::Ref(Box::new(IrType::Named("HashMap".to_string())))),
                    ("key".to_string(), IrType::U64),
                ],
                return_type: Some(IrType::Named("Option".to_string())),
            },
            CollectionFunction {
                name: "hashmap_remove".to_string(),
                params: vec![
                    ("map".to_string(), IrType::MutRef(Box::new(IrType::Named("HashMap".to_string())))),
                    ("key".to_string(), IrType::U64),
                ],
                return_type: Some(IrType::Named("Option".to_string())),
            },
            CollectionFunction {
                name: "hashmap_contains_key".to_string(),
                params: vec![
                    ("map".to_string(), IrType::Ref(Box::new(IrType::Named("HashMap".to_string())))),
                    ("key".to_string(), IrType::U64),
                ],
                return_type: Some(IrType::Bool),
            },
            CollectionFunction {
                name: "hashmap_len".to_string(),
                params: vec![("map".to_string(), IrType::Ref(Box::new(IrType::Named("HashMap".to_string()))))],
                return_type: Some(IrType::U64),
            },
            CollectionFunction {
                name: "hashmap_is_empty".to_string(),
                params: vec![("map".to_string(), IrType::Ref(Box::new(IrType::Named("HashMap".to_string()))))],
                return_type: Some(IrType::Bool),
            },
            CollectionFunction {
                name: "hashmap_clear".to_string(),
                params: vec![("map".to_string(), IrType::MutRef(Box::new(IrType::Named("HashMap".to_string()))))],
                return_type: None,
            },
            CollectionFunction {
                name: "hashmap_keys".to_string(),
                params: vec![("map".to_string(), IrType::Ref(Box::new(IrType::Named("HashMap".to_string()))))],
                return_type: Some(IrType::Named("Vec".to_string())),
            },
            CollectionFunction {
                name: "hashmap_values".to_string(),
                params: vec![("map".to_string(), IrType::Ref(Box::new(IrType::Named("HashMap".to_string()))))],
                return_type: Some(IrType::Named("Vec".to_string())),
            },
            CollectionFunction {
                name: "hashset_new".to_string(),
                params: vec![],
                return_type: Some(IrType::Named("HashSet".to_string())),
            },
            CollectionFunction {
                name: "hashset_with_capacity".to_string(),
                params: vec![("capacity".to_string(), IrType::U64)],
                return_type: Some(IrType::Named("HashSet".to_string())),
            },
            CollectionFunction {
                name: "hashset_insert".to_string(),
                params: vec![
                    ("set".to_string(), IrType::MutRef(Box::new(IrType::Named("HashSet".to_string())))),
                    ("value".to_string(), IrType::U64),
                ],
                return_type: Some(IrType::Bool),
            },
            CollectionFunction {
                name: "hashset_remove".to_string(),
                params: vec![
                    ("set".to_string(), IrType::MutRef(Box::new(IrType::Named("HashSet".to_string())))),
                    ("value".to_string(), IrType::U64),
                ],
                return_type: Some(IrType::Bool),
            },
            CollectionFunction {
                name: "hashset_contains".to_string(),
                params: vec![
                    ("set".to_string(), IrType::Ref(Box::new(IrType::Named("HashSet".to_string())))),
                    ("value".to_string(), IrType::U64),
                ],
                return_type: Some(IrType::Bool),
            },
            CollectionFunction {
                name: "hashset_len".to_string(),
                params: vec![("set".to_string(), IrType::Ref(Box::new(IrType::Named("HashSet".to_string()))))],
                return_type: Some(IrType::U64),
            },
            CollectionFunction {
                name: "hashset_is_empty".to_string(),
                params: vec![("set".to_string(), IrType::Ref(Box::new(IrType::Named("HashSet".to_string()))))],
                return_type: Some(IrType::Bool),
            },
            CollectionFunction {
                name: "hashset_clear".to_string(),
                params: vec![("set".to_string(), IrType::MutRef(Box::new(IrType::Named("HashSet".to_string()))))],
                return_type: None,
            },
            CollectionFunction {
                name: "hashset_union".to_string(),
                params: vec![
                    ("a".to_string(), IrType::Ref(Box::new(IrType::Named("HashSet".to_string())))),
                    ("b".to_string(), IrType::Ref(Box::new(IrType::Named("HashSet".to_string())))),
                ],
                return_type: Some(IrType::Named("HashSet".to_string())),
            },
            CollectionFunction {
                name: "hashset_intersection".to_string(),
                params: vec![
                    ("a".to_string(), IrType::Ref(Box::new(IrType::Named("HashSet".to_string())))),
                    ("b".to_string(), IrType::Ref(Box::new(IrType::Named("HashSet".to_string())))),
                ],
                return_type: Some(IrType::Named("HashSet".to_string())),
            },
            CollectionFunction {
                name: "hashset_difference".to_string(),
                params: vec![
                    ("a".to_string(), IrType::Ref(Box::new(IrType::Named("HashSet".to_string())))),
                    ("b".to_string(), IrType::Ref(Box::new(IrType::Named("HashSet".to_string())))),
                ],
                return_type: Some(IrType::Named("HashSet".to_string())),
            },
            CollectionFunction {
                name: "option_some".to_string(),
                params: vec![("value".to_string(), IrType::U64)],
                return_type: Some(IrType::Named("Option".to_string())),
            },
            CollectionFunction {
                name: "option_none".to_string(),
                params: vec![],
                return_type: Some(IrType::Named("Option".to_string())),
            },
            CollectionFunction {
                name: "option_is_some".to_string(),
                params: vec![("opt".to_string(), IrType::Ref(Box::new(IrType::Named("Option".to_string()))))],
                return_type: Some(IrType::Bool),
            },
            CollectionFunction {
                name: "option_is_none".to_string(),
                params: vec![("opt".to_string(), IrType::Ref(Box::new(IrType::Named("Option".to_string()))))],
                return_type: Some(IrType::Bool),
            },
            CollectionFunction {
                name: "option_unwrap".to_string(),
                params: vec![("opt".to_string(), IrType::Named("Option".to_string()))],
                return_type: Some(IrType::U64),
            },
            CollectionFunction {
                name: "option_unwrap_or".to_string(),
                params: vec![("opt".to_string(), IrType::Named("Option".to_string())), ("default".to_string(), IrType::U64)],
                return_type: Some(IrType::U64),
            },
            CollectionFunction {
                name: "option_map".to_string(),
                params: vec![
                    ("opt".to_string(), IrType::Named("Option".to_string())),
                    ("f".to_string(), IrType::Named("Function".to_string())),
                ],
                return_type: Some(IrType::Named("Option".to_string())),
            },
            CollectionFunction {
                name: "result_ok".to_string(),
                params: vec![("value".to_string(), IrType::U64)],
                return_type: Some(IrType::Named("Result".to_string())),
            },
            CollectionFunction {
                name: "result_err".to_string(),
                params: vec![("error".to_string(), IrType::U64)],
                return_type: Some(IrType::Named("Result".to_string())),
            },
            CollectionFunction {
                name: "result_is_ok".to_string(),
                params: vec![("res".to_string(), IrType::Ref(Box::new(IrType::Named("Result".to_string()))))],
                return_type: Some(IrType::Bool),
            },
            CollectionFunction {
                name: "result_is_err".to_string(),
                params: vec![("res".to_string(), IrType::Ref(Box::new(IrType::Named("Result".to_string()))))],
                return_type: Some(IrType::Bool),
            },
            CollectionFunction {
                name: "result_unwrap".to_string(),
                params: vec![("res".to_string(), IrType::Named("Result".to_string()))],
                return_type: Some(IrType::U64),
            },
            CollectionFunction {
                name: "result_unwrap_or".to_string(),
                params: vec![("res".to_string(), IrType::Named("Result".to_string())), ("default".to_string(), IrType::U64)],
                return_type: Some(IrType::U64),
            },
        ]
    }

    pub fn generate_assembly() -> String {
        let mut asm = String::new();

        asm.push_str("# CellScript Collections Library\n\n");
        asm.push_str(".section .text\n\n");
        asm.push_str(&Self::generate_fail_closed_helper());

        asm.push_str(&Self::generate_vec_impl());

        asm.push_str(&Self::generate_hashmap_impl());

        asm.push_str(&Self::generate_hashset_impl());

        asm
    }

    fn generate_fail_closed_helper() -> String {
        let mut asm = String::new();
        let error = CellScriptRuntimeError::CollectionBoundsInvalid;

        asm.push_str("# Collection runtime allocation is not part of the verifier ABI yet.\n");
        asm.push_str(".global __cellscript_collection_runtime_unavailable\n");
        asm.push_str("__cellscript_collection_runtime_unavailable:\n");
        asm.push_str(&format!("    # cellscript runtime error {} {}\n", error.code(), error.name()));
        asm.push_str(&format!("    li a0, {}\n", error.code()));
        asm.push_str(".Lcellscript_collection_runtime_unavailable:\n");
        asm.push_str("    j .Lcellscript_collection_runtime_unavailable\n\n");

        asm
    }

    fn generate_vec_impl() -> String {
        let mut asm = String::new();

        asm.push_str("# Vec::new\n");
        asm.push_str(".global __vec_new\n");
        asm.push_str("__vec_new:\n");
        asm.push_str("    j __cellscript_collection_runtime_unavailable\n\n");

        asm.push_str("# Vec::push\n");
        asm.push_str(".global __vec_push\n");
        asm.push_str("__vec_push:\n");
        asm.push_str("    j __cellscript_collection_runtime_unavailable\n\n");

        asm.push_str("# Vec::push unchecked body retained as non-entry documentation only\n");
        asm.push_str(".Lvec_push_unreachable_body:\n");
        asm.push_str("    addi sp, sp, -32\n");
        asm.push_str("    sd ra, 24(sp)\n");
        asm.push_str("    sd s0, 16(sp)\n");
        asm.push_str("    sd s1, 8(sp)\n");
        asm.push_str("    mv s0, a0          # vec pointer\n");
        asm.push_str("    mv s1, a1          # value to push\n");
        asm.push_str("    ld t0, 0(s0)       # capacity\n");
        asm.push_str("    ld t1, 8(s0)       # length\n");
        asm.push_str("    bgeu t1, t0, .Lvec_push_grow\n");
        asm.push_str(".Lvec_push_insert:\n");
        asm.push_str("    ld t2, 16(s0)      # data pointer\n");
        asm.push_str("    slli t3, t1, 3     # offset = length * 8\n");
        asm.push_str("    add t2, t2, t3\n");
        asm.push_str("    sd s1, 0(t2)       # store value\n");
        asm.push_str("    addi t1, t1, 1\n");
        asm.push_str("    sd t1, 8(s0)       # length++\n");
        asm.push_str("    ld ra, 24(sp)\n");
        asm.push_str("    ld s0, 16(sp)\n");
        asm.push_str("    ld s1, 8(sp)\n");
        asm.push_str("    addi sp, sp, 32\n");
        asm.push_str("    ret\n");
        asm.push_str(".Lvec_push_grow:\n");
        asm.push_str("    # Grow capacity (simplified: double)\n");
        asm.push_str("    beqz t0, .Lvec_push_init\n");
        asm.push_str("    slli t0, t0, 1\n");
        asm.push_str("    j .Lvec_push_alloc\n");
        asm.push_str(".Lvec_push_init:\n");
        asm.push_str("    li t0, 4           # initial capacity\n");
        asm.push_str(".Lvec_push_alloc:\n");
        asm.push_str("    sd t0, 0(s0)       # update capacity\n");
        asm.push_str("    slli a0, t0, 3     # new_size = capacity * 8\n");
        asm.push_str("    j __cellscript_collection_runtime_unavailable\n");
        asm.push_str("    sd a0, 16(s0)      # update data pointer\n");
        asm.push_str("    j .Lvec_push_insert\n\n");

        asm.push_str("# Vec::len\n");
        asm.push_str(".global __vec_len\n");
        asm.push_str("__vec_len:\n");
        asm.push_str("    j __cellscript_collection_runtime_unavailable\n\n");

        asm.push_str("# Vec::is_empty\n");
        asm.push_str(".global __vec_is_empty\n");
        asm.push_str("__vec_is_empty:\n");
        asm.push_str("    j __cellscript_collection_runtime_unavailable\n\n");

        asm
    }

    fn generate_hashmap_impl() -> String {
        let mut asm = String::new();

        asm.push_str("# HashMap::new\n");
        asm.push_str(".global __hashmap_new\n");
        asm.push_str("__hashmap_new:\n");
        asm.push_str("    j __cellscript_collection_runtime_unavailable\n\n");

        asm.push_str("# HashMap::insert\n");
        asm.push_str(".global __hashmap_insert\n");
        asm.push_str("__hashmap_insert:\n");
        asm.push_str("    j __cellscript_collection_runtime_unavailable\n\n");

        asm.push_str("# HashMap::get\n");
        asm.push_str(".global __hashmap_get\n");
        asm.push_str("__hashmap_get:\n");
        asm.push_str("    j __cellscript_collection_runtime_unavailable\n\n");

        asm.push_str("# HashMap::len\n");
        asm.push_str(".global __hashmap_len\n");
        asm.push_str("__hashmap_len:\n");
        asm.push_str("    j __cellscript_collection_runtime_unavailable\n\n");

        asm
    }

    fn generate_hashset_impl() -> String {
        let mut asm = String::new();

        asm.push_str("# HashSet::new\n");
        asm.push_str(".global __hashset_new\n");
        asm.push_str("__hashset_new:\n");
        asm.push_str("    j __hashmap_new    # HashSet is HashMap with unit values\n\n");

        asm.push_str("# HashSet::insert\n");
        asm.push_str(".global __hashset_insert\n");
        asm.push_str("__hashset_insert:\n");
        asm.push_str("    li a2, 1           # value = 1 (unit)\n");
        asm.push_str("    j __hashmap_insert\n\n");

        asm.push_str("# HashSet::contains\n");
        asm.push_str(".global __hashset_contains\n");
        asm.push_str("__hashset_contains:\n");
        asm.push_str("    j __cellscript_collection_runtime_unavailable\n\n");

        asm
    }
}

#[derive(Debug, Clone)]
pub struct CollectionFunction {
    pub name: String,
    pub params: Vec<(String, IrType)>,
    pub return_type: Option<IrType>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    #[test]
    fn test_collection_functions() {
        let funcs = Collections::functions();

        assert!(funcs.iter().any(|f| f.name == "vec_new"));
        assert!(funcs.iter().any(|f| f.name == "vec_push"));
        assert!(funcs.iter().any(|f| f.name == "vec_pop"));

        assert!(funcs.iter().any(|f| f.name == "hashmap_new"));
        assert!(funcs.iter().any(|f| f.name == "hashmap_insert"));
        assert!(funcs.iter().any(|f| f.name == "hashmap_get"));

        assert!(funcs.iter().any(|f| f.name == "hashset_new"));
        assert!(funcs.iter().any(|f| f.name == "hashset_insert"));
        assert!(funcs.iter().any(|f| f.name == "hashset_contains"));
    }

    #[test]
    fn test_generate_assembly() {
        let asm = Collections::generate_assembly();
        assert!(asm.contains("__vec_new"));
        assert!(asm.contains("__hashmap_new"));
        assert!(asm.contains("__hashmap_get"));
        assert!(asm.contains("__hashset_new"));

        let labels: BTreeSet<&str> = asm.lines().filter_map(|line| line.trim().strip_suffix(':')).collect();
        for line in asm.lines() {
            let instruction = line.split('#').next().unwrap_or_default().trim();
            if let Some(target) = instruction.strip_prefix("call ") {
                let target = target.trim();
                assert!(labels.contains(target), "generated collection assembly calls missing label {target}");
            }
        }
    }

    #[test]
    fn collection_assembly_has_no_raw_syscalls_or_unclassified_helpers() {
        let asm = Collections::generate_assembly();
        assert!(!asm.lines().any(|line| line.split('#').next().unwrap_or_default().trim() == "ecall"), "{asm}");

        for symbol in asm.lines().filter_map(|line| line.strip_prefix(".global ")) {
            assert!(
                crate::syscalls::helper_inventory_entry(symbol, crate::TargetProfile::Ckb).is_some(),
                "generated collection helper {symbol} must be classified"
            );
        }
    }

    #[test]
    fn collection_public_helpers_do_not_dereference_raw_a0_handles() {
        let asm = Collections::generate_assembly();
        let mut pending_global = None::<&str>;
        let mut current_global = None::<&str>;

        for line in asm.lines() {
            let trimmed = line.trim();
            if let Some(symbol) = trimmed.strip_prefix(".global ") {
                pending_global = Some(symbol);
                current_global = None;
                continue;
            }
            if let Some(label) = trimmed.strip_suffix(':') {
                if !label.starts_with(".L") {
                    current_global = if pending_global == Some(label) { pending_global } else { None };
                    pending_global = None;
                }
                continue;
            }

            let instruction = trimmed.split('#').next().unwrap_or_default().trim();
            if current_global.is_some() {
                assert!(
                    !instruction.contains("(a0)"),
                    "public collection helper {} must not dereference an unchecked raw a0 handle: {}",
                    current_global.unwrap_or("<unknown>"),
                    instruction
                );
            }
        }
    }
}
