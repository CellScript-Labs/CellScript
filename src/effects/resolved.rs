//! Compiler inference over resolved callable owners and argument sources.
//!
//! Inputs are established by typing/binding analysis. This module does not
//! discover targets by spelling and does not establish artifact admission. The
//! independent checker reconstructs facts from its own bound operation evidence.
use cellscript_artifact_checker::effect_facts::{CanonicalSet, ContextSource, EffectFacts, FactSetError};
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CallableId {
    pub owner: String,
    pub name: String,
}

/// A confirmed ordinary value has no transaction source. A source-bearing
/// argument retains all resolved alternatives, including caller parameters.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ArgumentSources {
    Value,
    Sources(CanonicalSet<ContextSource>),
}

#[derive(Debug, Clone)]
pub struct ResolvedCall {
    pub target: CallableId,
    pub arguments: Vec<ArgumentSources>,
}

#[derive(Debug, Clone)]
pub struct ResolvedCallable {
    pub id: CallableId,
    pub parameter_count: u16,
    pub direct: EffectFacts,
    pub calls: Vec<ResolvedCall>,
}

#[derive(Debug, Clone, Default)]
pub struct ResolvedFactGraph {
    nodes: BTreeMap<CallableId, ResolvedCallable>,
}

impl ResolvedFactGraph {
    pub fn insert(&mut self, callable: ResolvedCallable) -> Result<(), InferenceError> {
        use std::collections::btree_map::Entry;
        match self.nodes.entry(callable.id.clone()) {
            Entry::Vacant(entry) => {
                entry.insert(callable);
                Ok(())
            }
            Entry::Occupied(_) => Err(InferenceError::DuplicateCallable(callable.id)),
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct InferenceBudgets {
    pub max_callables: usize,
    pub max_calls: usize,
    pub max_argument_bindings: usize,
    pub max_steps: usize,
}

impl Default for InferenceBudgets {
    fn default() -> Self {
        Self { max_callables: 4096, max_calls: 65_536, max_argument_bindings: 65_536, max_steps: 1_000_000 }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InferenceError {
    DuplicateCallable(CallableId),
    UnknownCallee { caller: CallableId, target: CallableId },
    Arity { caller: CallableId, target: CallableId, expected: u16, actual: usize },
    InvalidParameter { owner: CallableId, parameter: u16 },
    MissingSource { caller: CallableId, target: CallableId, parameter: u16 },
    EmptySourceEvidence { caller: CallableId, target: CallableId },
    Budget(&'static str),
    Fact(FactSetError),
}

impl From<FactSetError> for InferenceError {
    fn from(error: FactSetError) -> Self {
        Self::Fact(error)
    }
}

impl fmt::Display for InferenceError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DuplicateCallable(id) => write!(formatter, "duplicate effect callable {}::{}", id.owner, id.name),
            Self::UnknownCallee { caller, target } => write!(
                formatter,
                "effect callee {}::{} from {}::{} has no resolved definition",
                target.owner, target.name, caller.owner, caller.name
            ),
            Self::Arity { caller, target, expected, actual } => write!(
                formatter,
                "effect edge {}::{} -> {}::{} expects {expected} arguments, found {actual}",
                caller.owner, caller.name, target.owner, target.name
            ),
            Self::InvalidParameter { owner, parameter } => {
                write!(formatter, "effect source parameter {parameter} is outside {}::{}", owner.owner, owner.name)
            }
            Self::MissingSource { caller, target, parameter } => write!(
                formatter,
                "effect edge {}::{} -> {}::{} has no source evidence for parameter {parameter}",
                caller.owner, caller.name, target.owner, target.name
            ),
            Self::EmptySourceEvidence { caller, target } => write!(
                formatter,
                "effect edge {}::{} -> {}::{} has empty source evidence",
                caller.owner, caller.name, target.owner, target.name
            ),
            Self::Budget(dimension) => write!(formatter, "effect inference exceeded its {dimension} budget"),
            Self::Fact(error) => error.fmt(formatter),
        }
    }
}
impl std::error::Error for InferenceError {}

/// Only a completed fixed point can construct this result. Empty facts are
/// justified completed inference, never a default for an absent callable.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InferredFactContracts {
    contracts: BTreeMap<CallableId, EffectFacts>,
}

impl InferredFactContracts {
    pub fn get(&self, id: &CallableId) -> Option<&EffectFacts> {
        self.contracts.get(id)
    }
}

fn validate_parameter(owner: &ResolvedCallable, source: ContextSource) -> Result<(), InferenceError> {
    if let ContextSource::Parameter { index } = source
        && index >= owner.parameter_count
    {
        return Err(InferenceError::InvalidParameter { owner: owner.id.clone(), parameter: index });
    }
    Ok(())
}

fn instantiate(caller: &CallableId, call: &ResolvedCall, callee: &EffectFacts) -> Result<EffectFacts, InferenceError> {
    let mut result = EffectFacts::empty();
    for operation in callee.lifecycle_ops().iter() {
        result.include_lifecycle(*operation)?;
    }
    for operation in callee.process_ops().iter() {
        result.include_process(*operation)?;
    }
    for read in callee.context_reads().iter() {
        if let ContextSource::Parameter { index } = read.source {
            let Some(ArgumentSources::Sources(sources)) = call.arguments.get(usize::from(index)) else {
                return Err(InferenceError::MissingSource { caller: caller.clone(), target: call.target.clone(), parameter: index });
            };
            for source in sources.iter() {
                result.include_read(*source, read.access)?;
            }
        } else {
            result.include_read(read.source, read.access)?;
        }
    }
    Ok(result)
}

/// Resolve all edges and bounds before inference, then compute a monotonic,
/// deterministic work-list fixed point. Analysis errors return no contracts.
pub fn infer(graph: &ResolvedFactGraph, budgets: &InferenceBudgets) -> Result<InferredFactContracts, InferenceError> {
    if graph.nodes.len() > budgets.max_callables {
        return Err(InferenceError::Budget("callable"));
    }
    let mut call_count = 0usize;
    let mut argument_count = 0usize;
    let mut parents: BTreeMap<&CallableId, BTreeSet<&CallableId>> = BTreeMap::new();
    for node in graph.nodes.values() {
        node.direct.validate()?;
        for read in node.direct.context_reads().iter() {
            validate_parameter(node, read.source)?;
        }
        for call in &node.calls {
            call_count += 1;
            if call_count > budgets.max_calls {
                return Err(InferenceError::Budget("call edge"));
            }
            let Some(target) = graph.nodes.get(&call.target) else {
                return Err(InferenceError::UnknownCallee { caller: node.id.clone(), target: call.target.clone() });
            };
            if call.arguments.len() != usize::from(target.parameter_count) {
                return Err(InferenceError::Arity {
                    caller: node.id.clone(),
                    target: call.target.clone(),
                    expected: target.parameter_count,
                    actual: call.arguments.len(),
                });
            }
            for argument in &call.arguments {
                argument_count += 1;
                if argument_count > budgets.max_argument_bindings {
                    return Err(InferenceError::Budget("argument binding"));
                }
                if let ArgumentSources::Sources(sources) = argument {
                    if sources.is_empty() {
                        return Err(InferenceError::EmptySourceEvidence { caller: node.id.clone(), target: call.target.clone() });
                    }
                    argument_count = argument_count.checked_add(sources.len()).ok_or(InferenceError::Budget("argument binding"))?;
                    if argument_count > budgets.max_argument_bindings {
                        return Err(InferenceError::Budget("argument binding"));
                    }
                    for source in sources.iter() {
                        validate_parameter(node, *source)?;
                    }
                }
            }
            parents.entry(&call.target).or_default().insert(&node.id);
        }
    }
    let mut contracts = graph.nodes.iter().map(|(id, node)| (id.clone(), node.direct.clone())).collect::<BTreeMap<_, _>>();
    let mut queue = graph.nodes.keys().collect::<VecDeque<_>>();
    let mut queued = graph.nodes.keys().collect::<BTreeSet<_>>();
    let mut steps = 0usize;
    while let Some(id) = queue.pop_front() {
        queued.remove(id);
        steps += 1;
        if steps > budgets.max_steps {
            return Err(InferenceError::Budget("convergence"));
        }
        let node = &graph.nodes[id];
        let mut facts = node.direct.clone();
        for call in &node.calls {
            facts = facts.union(&instantiate(id, call, &contracts[&call.target])?)?;
        }
        if contracts[id] != facts {
            contracts.insert(id.clone(), facts);
            for parent in parents.get(id).into_iter().flatten() {
                if queued.insert(parent) {
                    queue.push_back(parent);
                }
            }
        }
    }
    Ok(InferredFactContracts { contracts })
}

#[cfg(test)]
mod tests {
    use super::*;
    use cellscript_artifact_checker::effect_facts::{ContextAccess as A, LifecycleOperation as L, ProcessOperation as P};
    fn id(owner: &str, name: &str) -> CallableId {
        CallableId { owner: owner.into(), name: name.into() }
    }
    fn node(id: CallableId, parameter_count: u16, direct: EffectFacts, calls: Vec<ResolvedCall>) -> ResolvedCallable {
        ResolvedCallable { id, parameter_count, direct, calls }
    }
    fn sources(values: &[ContextSource]) -> ArgumentSources {
        let mut sources = CanonicalSet::default();
        for source in values {
            sources.insert(*source).unwrap();
        }
        ArgumentSources::Sources(sources)
    }
    fn graph(nodes: Vec<ResolvedCallable>) -> ResolvedFactGraph {
        let mut graph = ResolvedFactGraph::default();
        for node in nodes {
            graph.insert(node).unwrap();
        }
        graph
    }
    fn run(nodes: Vec<ResolvedCallable>) -> Result<InferredFactContracts, InferenceError> {
        infer(&graph(nodes), &InferenceBudgets::default())
    }

    #[test]
    fn transitive_reads_creation_and_communication_keep_independent_facts() {
        let root = id("root", "run");
        let observe = id("dep", "observe");
        let pipe = id("dep", "send");
        let result = run(vec![
            node(
                root.clone(),
                0,
                EffectFacts::lifecycle(L::Create),
                vec![
                    ResolvedCall { target: observe.clone(), arguments: vec![sources(&[ContextSource::GroupInput])] },
                    ResolvedCall { target: pipe.clone(), arguments: vec![] },
                ],
            ),
            node(observe, 1, EffectFacts::read(ContextSource::Parameter { index: 0 }, A::Capacity), vec![]),
            node(pipe, 0, EffectFacts::process(P::PipeWrite), vec![]),
        ])
        .unwrap();
        let expected = EffectFacts::read(ContextSource::GroupInput, A::Capacity)
            .union(&EffectFacts::lifecycle(L::Create))
            .unwrap()
            .union(&EffectFacts::process(P::PipeWrite))
            .unwrap();
        assert_eq!(result.get(&root), Some(&expected));
        assert!(!EffectFacts::lifecycle(L::Create).accepts(&expected));
    }

    #[test]
    fn argument_permutation_and_definition_owners_survive_two_call_edges() {
        let inspect = id("records", "inspect");
        let same_name = id("processes", "inspect");
        let helper = id("mid", "helper");
        let root = id("root", "run");
        let result = run(vec![
            node(inspect.clone(), 2, EffectFacts::read(ContextSource::Parameter { index: 1 }, A::CellData), vec![]),
            node(same_name.clone(), 0, EffectFacts::process(P::Exec), vec![]),
            node(
                helper.clone(),
                1,
                EffectFacts::empty(),
                vec![ResolvedCall {
                    target: inspect,
                    arguments: vec![ArgumentSources::Value, sources(&[ContextSource::Parameter { index: 0 }])],
                }],
            ),
            node(
                root.clone(),
                0,
                EffectFacts::empty(),
                vec![ResolvedCall { target: helper, arguments: vec![sources(&[ContextSource::CellDep])] }],
            ),
        ])
        .unwrap();
        assert_eq!(result.get(&root), Some(&EffectFacts::read(ContextSource::CellDep, A::CellData)));
        assert_eq!(result.get(&same_name), Some(&EffectFacts::process(P::Exec)));
    }

    #[test]
    fn recursive_components_converge_without_escaping_empty_accumulators() {
        let left = id("m", "left");
        let right = id("m", "right");
        let edge = |target| ResolvedCall { target, arguments: vec![sources(&[ContextSource::Parameter { index: 0 }])] };
        let result = run(vec![
            node(left.clone(), 1, EffectFacts::read(ContextSource::Parameter { index: 0 }, A::Header), vec![edge(right.clone())]),
            node(right.clone(), 1, EffectFacts::process(P::PipeRead), vec![edge(left.clone())]),
        ])
        .unwrap();
        let expected =
            EffectFacts::read(ContextSource::Parameter { index: 0 }, A::Header).union(&EffectFacts::process(P::PipeRead)).unwrap();
        assert_eq!(result.get(&left), Some(&expected));
        assert_eq!(result.get(&right), Some(&expected));
    }

    #[test]
    fn ordinary_values_do_not_create_reads_but_cannot_satisfy_a_source_read() {
        let caller = id("m", "caller");
        let callee = id("m", "callee");
        let edge = ResolvedCall { target: callee.clone(), arguments: vec![ArgumentSources::Value] };
        let result = run(vec![
            node(caller.clone(), 0, EffectFacts::empty(), vec![edge.clone()]),
            node(callee.clone(), 1, EffectFacts::empty(), vec![]),
        ])
        .unwrap();
        assert!(result.get(&caller).unwrap().is_empty());
        assert_eq!(
            run(vec![
                node(caller.clone(), 0, EffectFacts::empty(), vec![edge]),
                node(callee.clone(), 1, EffectFacts::read(ContextSource::Parameter { index: 0 }, A::Witness), vec![])
            ]),
            Err(InferenceError::MissingSource { caller, target: callee, parameter: 0 })
        );
    }

    #[test]
    fn malformed_or_missing_bindings_return_no_completed_contracts() {
        let caller = id("m", "caller");
        let target = id("m", "target");
        assert!(matches!(
            run(vec![node(caller.clone(), 0, EffectFacts::empty(), vec![ResolvedCall { target: target.clone(), arguments: vec![] }])]),
            Err(InferenceError::UnknownCallee { .. })
        ));
        assert!(matches!(
            run(vec![node(caller.clone(), 0, EffectFacts::read(ContextSource::Parameter { index: 0 }, A::Witness), vec![])]),
            Err(InferenceError::InvalidParameter { .. })
        ));
        assert!(matches!(
            run(vec![
                node(caller.clone(), 0, EffectFacts::empty(), vec![ResolvedCall { target: target.clone(), arguments: vec![] }]),
                node(target.clone(), 1, EffectFacts::empty(), vec![])
            ]),
            Err(InferenceError::Arity { .. })
        ));
        assert!(matches!(
            run(vec![
                node(caller, 0, EffectFacts::empty(), vec![ResolvedCall { target: target.clone(), arguments: vec![sources(&[])] }]),
                node(target, 1, EffectFacts::empty(), vec![])
            ]),
            Err(InferenceError::EmptySourceEvidence { .. })
        ));
        let duplicate = node(id("m", "duplicate"), 0, EffectFacts::empty(), vec![]);
        let mut graph = ResolvedFactGraph::default();
        graph.insert(duplicate.clone()).unwrap();
        assert!(matches!(graph.insert(duplicate), Err(InferenceError::DuplicateCallable(_))));
    }

    #[test]
    fn inference_is_deterministic_and_reports_budgets_instead_of_a_partial_result() {
        let caller = id("m", "a");
        let target = id("m", "z");
        let nodes = vec![
            node(
                caller.clone(),
                0,
                EffectFacts::empty(),
                vec![ResolvedCall {
                    target: target.clone(),
                    arguments: vec![sources(&[ContextSource::Input, ContextSource::GroupInput])],
                }],
            ),
            node(target, 1, EffectFacts::read(ContextSource::Parameter { index: 0 }, A::CellData), vec![]),
        ];
        let mut reversed = nodes.clone();
        reversed.reverse();
        assert_eq!(run(nodes.clone()).unwrap(), run(reversed).unwrap());
        for budgets in [
            InferenceBudgets { max_callables: 1, ..Default::default() },
            InferenceBudgets { max_calls: 0, ..Default::default() },
            InferenceBudgets { max_argument_bindings: 0, ..Default::default() },
            InferenceBudgets { max_steps: 0, ..Default::default() },
        ] {
            assert!(matches!(infer(&graph(nodes.clone()), &budgets), Err(InferenceError::Budget(_))));
        }
        let result = run(nodes).unwrap();
        assert_eq!(result.get(&caller).unwrap().context_reads().len(), 2);
        assert!(result.get(&id("m", "missing")).is_none());
    }
}
