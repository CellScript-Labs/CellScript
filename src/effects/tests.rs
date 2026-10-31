use super::*;
use crate::resolve::ModuleResolver;
use cellscript_artifact_checker::effect_facts::{
    ContextAccess as Access, ContextSource as Source, LifecycleOperation as Life, ProcessOperation as Process,
};
use std::collections::BTreeSet;
fn parse(source: &str) -> Module {
    crate::frontend::parse(source, crate::CURRENT_EDITION).unwrap()
}
#[test]
fn infers_independent_dimensions_without_category_projection() {
    let module = parse(
        r#"module effects
resource Token has store, create, consume { amount: u64 }
fn compute(value: u64) -> u64 { return value + 1 }
fn observe() -> u64 { return ckb::cell_capacity(source::input(0)) }
action make(owner: Address) { verification create Token { amount: 1 } with_lock(owner) }
action spend(value: Token) { verification consume value }
action rebuild(value: Token, owner: Address) { verification consume value create Token { amount: 2 } with_lock(owner) }
"#,
    );
    let effects = infer_module(&module, None).unwrap();
    assert!(effects.get("compute").unwrap().is_empty());
    assert_eq!(effects.get("observe"), Some(EffectFacts::read(Source::Input, Access::Capacity)));
    for (name, expected) in [
        ("make", BTreeSet::from([Life::Create])),
        ("spend", BTreeSet::from([Life::Consume])),
        ("rebuild", BTreeSet::from([Life::Consume, Life::Create])),
    ] {
        let facts = effects.get(name).unwrap();
        assert_eq!(facts.lifecycle_ops().iter().copied().collect::<BTreeSet<_>>(), expected, "{name}");
        assert!(facts.process_ops().is_empty());
    }
}
#[test]
fn recursive_components_propagate_reads_without_assuming_return() {
    let module=parse("module recursive\nfn a() -> u64 { return b() }\nfn b() -> u64 { let value = ckb::cell_capacity(source::input(0)) return a() + value }\nfn wrapper() -> u64 { return a() }");
    let effects = infer_module(&module, None).unwrap();
    for name in ["a", "b", "wrapper"] {
        assert_eq!(effects.get(name), Some(EffectFacts::read(Source::Input, Access::Capacity)));
    }
}
#[test]
fn imported_aliases_retain_the_bound_definition() {
    let library = parse("module library\npublic fn query() -> u64 { return ckb::cell_capacity(source::input(0)) }");
    let app=parse("module app\nuse library::query as lookup\nfn local() -> u64 { return lookup() }\nfn qualified() -> u64 { return library::query() }");
    let mut resolver = ModuleResolver::new();
    resolver.register_module(library).unwrap();
    resolver.register_module(app.clone()).unwrap();
    let effects = infer_module(&app, Some(&resolver)).unwrap();
    for name in ["local", "qualified", "lookup", "library::query"] {
        assert_eq!(effects.get(name), Some(EffectFacts::read(Source::Input, Access::Capacity)), "{name}");
    }
}
#[test]
fn unresolved_targets_never_publish_empty_facts() {
    let error = infer_module(&parse("module unresolved\nfn wrapper() -> u64 { return unavailable() }"), None).unwrap_err();
    assert!(error.message.contains("call 'unavailable' has no known return type during IR lowering"), "{error}");
}
#[test]
fn local_mutation_and_static_type_identity_are_external_effect_free() {
    let module=parse("module local\nstruct Plain { value: u64 }\nfn compute(value: u64) -> u64 { let mut next = value next = next + 1 return next }\nfn identity(value: Plain) -> Hash { return value.type_hash() }");
    let facts = infer_module(&module, None).unwrap();
    assert!(facts.get("compute").unwrap().is_empty());
    assert!(facts.get("identity").unwrap().is_empty());
}
#[test]
fn process_operations_propagate_separately_from_cell_reads() {
    let module =
        parse("module ipc\nfn send(fd: u64) -> u64 { return pipe_write(fd, 7) }\nfn forward(fd: u64) -> u64 { return send(fd) }");
    let facts = infer_module(&module, None).unwrap();
    assert_eq!(facts.get("forward"), Some(EffectFacts::process(Process::PipeWrite)));
    assert!(!facts.get("forward").unwrap().allows_observation());
}
#[test]
fn call_substitution_preserves_all_source_alternatives() {
    let module=parse("module bindings\nresource Token has store { amount: u64 }\nfn capacity(view: InputView<Token>) -> u64 { return ckb::cell_capacity(view) }\nfn wrapper() -> u64 { return capacity(ckb::group_input<Token>(0)) }");
    let facts = infer_module(&module, None).unwrap();
    assert_eq!(facts.get("capacity"), Some(EffectFacts::read(Source::Parameter { index: 0 }, Access::Capacity)));
    assert_eq!(facts.get("wrapper"), Some(EffectFacts::read(Source::GroupInput, Access::Capacity)));
}
