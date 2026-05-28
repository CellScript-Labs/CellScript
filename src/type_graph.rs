use crate::error::{CompileError, Result, Span};
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub(crate) struct TypeDependencyEdge {
    pub(crate) target: String,
    pub(crate) span: Span,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TypeGraphVisitState {
    Visiting,
    Done,
}

pub(crate) fn visit_type_dependency_graph(
    name: &str,
    graph: &HashMap<String, Vec<TypeDependencyEdge>>,
    states: &mut HashMap<String, TypeGraphVisitState>,
    stack: &mut Vec<String>,
) -> Result<()> {
    states.insert(name.to_string(), TypeGraphVisitState::Visiting);
    stack.push(name.to_string());

    if let Some(edges) = graph.get(name) {
        for edge in edges {
            match states.get(&edge.target).copied() {
                Some(TypeGraphVisitState::Done) => {}
                Some(TypeGraphVisitState::Visiting) => {
                    let start = stack.iter().position(|entry| entry == &edge.target).unwrap_or(0);
                    let mut cycle = stack[start..].to_vec();
                    cycle.push(edge.target.clone());
                    return Err(CompileError::new(format!("cyclic type dependency detected: {}", cycle.join(" -> ")), edge.span));
                }
                None => visit_type_dependency_graph(&edge.target, graph, states, stack)?,
            }
        }
    }

    stack.pop();
    states.insert(name.to_string(), TypeGraphVisitState::Done);
    Ok(())
}
