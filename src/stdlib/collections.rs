//! Retired standalone allocation-helper assembly. The String generator and all
//! catalogued symbols remain available for compatibility, but every invocation
//! terminates with CollectionRuntimeUnsupported. These historical symbols never
//! implement the ordinary compiler's checked stack/arena collection lowering.

use crate::ir::IrType;

pub struct Collections;

impl Collections {
    pub fn functions() -> Vec<CollectionFunction> {
        vec![
            CollectionFunction { name: "vec_new".to_string(), params: vec![], return_type: Some(IrType::Vector(None)) },
            CollectionFunction {
                name: "vec_with_capacity".to_string(),
                params: vec![("capacity".to_string(), IrType::U64)],
                return_type: Some(IrType::Vector(None)),
            },
            CollectionFunction {
                name: "vec_push".to_string(),
                params: vec![("vec".to_string(), IrType::MutRef(Box::new(IrType::Vector(None)))), ("value".to_string(), IrType::U64)],
                return_type: None,
            },
            CollectionFunction {
                name: "vec_pop".to_string(),
                params: vec![("vec".to_string(), IrType::MutRef(Box::new(IrType::Vector(None))))],
                return_type: Some(IrType::Named("Option".to_string())),
            },
            CollectionFunction {
                name: "vec_get".to_string(),
                params: vec![("vec".to_string(), IrType::Ref(Box::new(IrType::Vector(None)))), ("index".to_string(), IrType::U64)],
                return_type: Some(IrType::Named("Option".to_string())),
            },
            CollectionFunction {
                name: "vec_len".to_string(),
                params: vec![("vec".to_string(), IrType::Ref(Box::new(IrType::Vector(None))))],
                return_type: Some(IrType::U64),
            },
            CollectionFunction {
                name: "vec_is_empty".to_string(),
                params: vec![("vec".to_string(), IrType::Ref(Box::new(IrType::Vector(None))))],
                return_type: Some(IrType::Bool),
            },
            CollectionFunction {
                name: "vec_clear".to_string(),
                params: vec![("vec".to_string(), IrType::MutRef(Box::new(IrType::Vector(None))))],
                return_type: None,
            },
            CollectionFunction {
                name: "vec_contains".to_string(),
                params: vec![("vec".to_string(), IrType::Ref(Box::new(IrType::Vector(None)))), ("value".to_string(), IrType::U64)],
                return_type: Some(IrType::Bool),
            },
            CollectionFunction {
                name: "vec_remove".to_string(),
                params: vec![("vec".to_string(), IrType::MutRef(Box::new(IrType::Vector(None)))), ("index".to_string(), IrType::U64)],
                return_type: Some(IrType::U64),
            },
            CollectionFunction {
                name: "vec_insert".to_string(),
                params: vec![
                    ("vec".to_string(), IrType::MutRef(Box::new(IrType::Vector(None)))),
                    ("index".to_string(), IrType::U64),
                    ("value".to_string(), IrType::U64),
                ],
                return_type: None,
            },
            CollectionFunction {
                name: "vec_sort".to_string(),
                params: vec![("vec".to_string(), IrType::MutRef(Box::new(IrType::Vector(None))))],
                return_type: None,
            },
            CollectionFunction {
                name: "vec_reverse".to_string(),
                params: vec![("vec".to_string(), IrType::MutRef(Box::new(IrType::Vector(None))))],
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
                return_type: Some(IrType::Vector(None)),
            },
            CollectionFunction {
                name: "hashmap_values".to_string(),
                params: vec![("map".to_string(), IrType::Ref(Box::new(IrType::Named("HashMap".to_string()))))],
                return_type: Some(IrType::Vector(None)),
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

    /// Preserve symbol discovery without returning success for unimplemented
    /// collection semantics or issuing a fictional allocation syscall.
    pub fn generate_assembly() -> String {
        let mut asm = "# Retired CellScript standalone collection helpers\n.section .text\n".to_string();
        for function in Self::functions() {
            let symbol = format!("__{}", function.name);
            asm.push_str(&format!(".global {symbol}\n{symbol}:\n"));
            super::push_abort(&mut asm, &symbol, crate::runtime_errors::CellScriptRuntimeError::CollectionRuntimeUnsupported);
        }
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
}
