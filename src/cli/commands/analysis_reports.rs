use crate::{CompileMetadata, ProofPlanMetadata};
use std::collections::{BTreeMap, BTreeSet};

pub(super) fn proof_diff_report(old: &CompileMetadata, new: &CompileMetadata) -> serde_json::Value {
    let old_map = proof_plan_map(&old.runtime.proof_plan);
    let new_map = proof_plan_map(&new.runtime.proof_plan);
    let old_keys = old_map.keys().cloned().collect::<BTreeSet<_>>();
    let new_keys = new_map.keys().cloned().collect::<BTreeSet<_>>();
    let added = new_keys.difference(&old_keys).cloned().collect::<Vec<_>>();
    let removed = old_keys.difference(&new_keys).cloned().collect::<Vec<_>>();
    let changed = old_keys.intersection(&new_keys).filter(|key| old_map.get(*key) != new_map.get(*key)).cloned().collect::<Vec<_>>();
    serde_json::json!({
        "status": "ok",
        "schema": "cellscript-proof-diff-v0.16",
        "old_module": old.module,
        "new_module": new.module,
        "added": added,
        "removed": removed,
        "changed": changed,
    })
}

fn proof_plan_map(plans: &[ProofPlanMetadata]) -> BTreeMap<String, serde_json::Value> {
    plans
        .iter()
        .map(|plan| {
            (
                format!("{}:{}:{}", plan.origin, plan.feature, plan.status),
                serde_json::json!({
                    "trigger": plan.trigger,
                    "scope": plan.scope,
                    "reads": plan.reads,
                    "coverage": plan.coverage,
                    "codegen_coverage_status": plan.codegen_coverage_status,
                    "on_chain_checked": plan.on_chain_checked,
                }),
            )
        })
        .collect()
}

pub(super) fn json_diff_report(kind: &str, old: &serde_json::Value, new: &serde_json::Value) -> serde_json::Value {
    let changed = [
        "/artifact/hash",
        "/artifact/size_bytes",
        "/target_profile/name",
        "/proof_plan_soundness/status",
        "/metadata_schema_version",
        "/metadata_schema_versions/metadata",
        "/metadata_schema_versions/source",
        "/metadata_schema_versions/artifact",
        "/metadata_schema_versions/constraints",
    ]
    .iter()
    .filter(|pointer| old.pointer(pointer) != new.pointer(pointer))
    .map(|pointer| {
        serde_json::json!({
            "path": pointer,
            "old": old.pointer(pointer).cloned().unwrap_or(serde_json::Value::Null),
            "new": new.pointer(pointer).cloned().unwrap_or(serde_json::Value::Null),
        })
    })
    .collect::<Vec<_>>();
    serde_json::json!({
        "status": "ok",
        "schema": format!("cellscript-{}-diff-v0.16", kind),
        "changed": changed,
    })
}

pub(super) fn profile_report_json(metadata: &CompileMetadata, entry: Option<&str>) -> serde_json::Value {
    let mut proof_plan_records = Vec::new();
    let actions = metadata
        .actions
        .iter()
        .filter(|action| entry.is_none_or(|entry| action.name == entry))
        .map(|action| {
            proof_plan_records.extend(action.proof_plan.iter().map(|plan| {
                profile_proof_plan_record_json(
                    "action",
                    &action.name,
                    serde_json::json!(action.estimated_cycles),
                    action.ckb_runtime_accesses.len(),
                    plan,
                )
            }));
            serde_json::json!({
                "kind": "action",
                "name": action.name,
                "estimated_cycles": action.estimated_cycles,
                "proof_plan_records": action.proof_plan.len(),
                "runtime_accesses": action.ckb_runtime_accesses.len(),
            })
        })
        .collect::<Vec<_>>();
    let locks = metadata
        .locks
        .iter()
        .filter(|lock| entry.is_none_or(|entry| lock.name == entry))
        .map(|lock| {
            proof_plan_records.extend(lock.proof_plan.iter().map(|plan| {
                profile_proof_plan_record_json("lock", &lock.name, serde_json::Value::Null, lock.ckb_runtime_accesses.len(), plan)
            }));
            serde_json::json!({
                "kind": "lock",
                "name": lock.name,
                "estimated_cycles": null,
                "proof_plan_records": lock.proof_plan.len(),
                "runtime_accesses": lock.ckb_runtime_accesses.len(),
            })
        })
        .collect::<Vec<_>>();
    serde_json::json!({
        "status": "ok",
        "schema": "cellscript-profile-v0.16",
        "module": metadata.module,
        "entry": entry,
        "actions": actions,
        "locks": locks,
        "proof_plan_records": proof_plan_records,
        "proof_plan_soundness": metadata.runtime.proof_plan_soundness,
    })
}

fn profile_proof_plan_record_json(
    entry_kind: &str,
    entry_name: &str,
    estimated_cycles: serde_json::Value,
    runtime_accesses: usize,
    plan: &ProofPlanMetadata,
) -> serde_json::Value {
    serde_json::json!({
        "entry_kind": entry_kind,
        "entry_name": entry_name,
        "name": plan.name,
        "origin": plan.origin,
        "category": plan.category,
        "feature": plan.feature,
        "trigger": plan.trigger,
        "scope": plan.scope,
        "reads": plan.reads,
        "coverage": plan.coverage,
        "codegen_coverage_status": plan.codegen_coverage_status,
        "on_chain_checked": plan.on_chain_checked,
        "status": plan.status,
        "estimated_cycles": estimated_cycles,
        "runtime_accesses": runtime_accesses,
        "builder_assumptions": plan.builder_assumptions,
        "detail": plan.detail,
    })
}

pub(super) fn trace_tx_report_json(metadata: &CompileMetadata, validation: &crate::TxValidationReport) -> serde_json::Value {
    serde_json::json!({
        "status": validation.status,
        "schema": "cellscript-tx-trace-v0.16",
        "module": metadata.module,
        "steps": metadata.runtime.builder_assumptions.iter().map(|assumption| {
            serde_json::json!({
                "assumption_id": assumption.assumption_id,
                "kind": assumption.kind,
                "origin": assumption.origin,
                "feature": assumption.feature,
                "checked": validation.checked_assumptions.contains(&assumption.assumption_id),
            })
        }).collect::<Vec<_>>(),
        "validation": validation,
    })
}

pub(super) fn protocol_graph_json(metadata: &CompileMetadata) -> serde_json::Value {
    let mut vertices = BTreeMap::<String, serde_json::Value>::new();
    let type_names = metadata.types.iter().map(|ty| ty.name.clone()).collect::<BTreeSet<_>>();

    for ty in &metadata.types {
        if ty.flow_states.is_empty() {
            protocol_graph_insert_vertex(
                &mut vertices,
                protocol_graph_type_vertex_id(&ty.name),
                &ty.name,
                &ty.kind,
                Some(&ty.name),
                None,
                false,
                false,
                false,
            );
        } else {
            for state in &ty.flow_states {
                protocol_graph_insert_vertex(
                    &mut vertices,
                    protocol_graph_state_vertex_id(&ty.name, state),
                    &format!("{}[{}]", ty.name, state),
                    &ty.kind,
                    Some(&ty.name),
                    Some(state),
                    false,
                    ty.flow_initial_state.as_deref() == Some(state.as_str()),
                    ty.flow_terminal_states.iter().any(|terminal| terminal == state),
                );
            }
        }
    }

    let mut edges = Vec::new();
    let mut seen_edges = BTreeSet::new();
    for action in &metadata.actions {
        for edge in &action.state_transition_edges {
            let source = protocol_graph_state_vertex_id(&edge.type_name, &edge.from);
            let target = protocol_graph_state_vertex_id(&edge.type_name, &edge.to);
            protocol_graph_push_edge(
                &mut edges,
                &mut seen_edges,
                action,
                source,
                target,
                "state-transition",
                Some(serde_json::json!({
                    "type_name": &edge.type_name,
                    "field_name": &edge.field_name,
                    "from": &edge.from,
                    "to": &edge.to,
                    "from_index": edge.from_index,
                    "to_index": edge.to_index,
                    "input_binding": &edge.input_binding,
                    "output_binding": &edge.output_binding,
                })),
            );
        }

        if action.state_transition_edges.is_empty() {
            let (sources, targets) = protocol_graph_type_pattern_vertices(action, &type_names);
            for source in sources {
                protocol_graph_ensure_vertex(&mut vertices, &source);
                for target in &targets {
                    protocol_graph_ensure_vertex(&mut vertices, target);
                    protocol_graph_push_edge(
                        &mut edges,
                        &mut seen_edges,
                        action,
                        source.clone(),
                        target.clone(),
                        "type-pattern",
                        None,
                    );
                }
            }
        }
    }

    let vertex_values = vertices.values().cloned().collect::<Vec<_>>();
    let cycle_detected = protocol_graph_has_cycle(&edges);
    let self_loop_count = edges.iter().filter(|edge| edge.get("source_vertex") == edge.get("target_vertex")).count();
    let role_lints = protocol_graph_role_lints(&edges);

    serde_json::json!({
        "status": "ok",
        "schema": "cellscript-protocol-graph-v0.22",
        "derivation": "derived-from-compile-metadata",
        "consensus_checked": false,
        "module": metadata.module,
        "target_profile": metadata.target_profile.name,
        "vertex_count": vertex_values.len(),
        "edge_count": edges.len(),
        "cycle_detected": cycle_detected,
        "self_loop_count": self_loop_count,
        "role_model": {
            "semantics": "explanatory-metadata-only",
            "evidence_tier": "metadata-only",
            "authorization_proven": false,
            "source_precedence": ["verification-predicate", "witness-or-lock-args-binding", "field-name"],
        },
        "role_warning_count": role_lints.len(),
        "role_lints": role_lints,
        "vertices": vertex_values,
        "edges": edges,
    })
}

fn protocol_graph_insert_vertex(
    vertices: &mut BTreeMap<String, serde_json::Value>,
    id: String,
    label: &str,
    kind: &str,
    type_name: Option<&str>,
    state: Option<&str>,
    synthetic: bool,
    initial: bool,
    terminal: bool,
) {
    vertices.entry(id.clone()).or_insert_with(|| {
        serde_json::json!({
            "id": id,
            "label": label,
            "kind": kind,
            "type_name": type_name,
            "state": state,
            "synthetic": synthetic,
            "initial": initial,
            "terminal": terminal,
        })
    });
}

fn protocol_graph_ensure_vertex(vertices: &mut BTreeMap<String, serde_json::Value>, id: &str) {
    if vertices.contains_key(id) {
        return;
    }
    let (label, kind) = match id {
        "transaction:start" => ("transaction start", "transaction-boundary"),
        "transaction:end" => ("transaction end", "transaction-boundary"),
        other => (other, "type"),
    };
    protocol_graph_insert_vertex(vertices, id.to_string(), label, kind, None, None, true, false, false);
}

fn protocol_graph_push_edge(
    edges: &mut Vec<serde_json::Value>,
    seen: &mut BTreeSet<(String, String, String, String)>,
    action: &crate::ActionMetadata,
    source: String,
    target: String,
    derivation: &str,
    state_transition: Option<serde_json::Value>,
) {
    let key = (action.name.clone(), source.clone(), target.clone(), derivation.to_string());
    if !seen.insert(key) {
        return;
    }
    let proof_plan_ids = action.proof_plan.iter().map(|plan| format!("{}:{}", plan.origin, plan.feature)).collect::<Vec<_>>();
    let builder_assumptions = action
        .proof_plan
        .iter()
        .flat_map(|plan| plan.builder_assumptions.iter().cloned())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    let source_span = action.proof_plan.iter().find_map(|plan| plan.source_span.clone());
    let role = protocol_graph_role_view(action);

    edges.push(serde_json::json!({
        "action_name": &action.name,
        "source_vertex": source,
        "target_vertex": target,
        "derivation": derivation,
        "state_transition": state_transition,
        "consume_set": &action.consume_set,
        "read_refs": &action.read_refs,
        "create_set": &action.create_set,
        "mutate_set": &action.mutate_set,
        "proof_plan_ids": proof_plan_ids,
        "ckb_runtime_accesses": &action.ckb_runtime_accesses,
        "builder_assumptions": builder_assumptions,
        "touches_shared": &action.touches_shared,
        "source_span": source_span,
        "role": role["role"].clone(),
        "role_source": role["role_source"].clone(),
        "role_strength": role["role_strength"].clone(),
        "role_status": role["role_status"].clone(),
        "role_source_used": role["role_source_used"].clone(),
        "role_candidates": role["role_candidates"].clone(),
        "role_conflict": role["role_conflict"].clone(),
        "role_warnings": role["role_warnings"].clone(),
        "role_evidence_tier": "metadata-only",
        "authorization_proven": false,
    }));
}

fn protocol_graph_role_view(action: &crate::ActionMetadata) -> serde_json::Value {
    let selected = action.protocol_role_candidates.first();
    let roles = action.protocol_role_candidates.iter().map(|candidate| candidate.role.as_str()).collect::<BTreeSet<_>>();
    let conflict = roles.len() > 1;
    let mut warnings = Vec::new();

    if selected.is_none() {
        warnings.push(serde_json::json!({
            "code": "PG-ROLE-MISSING",
            "severity": "warning",
            "action_name": &action.name,
            "message": format!(
                "action '{}' has no explicit predicate, witness/lock_args binding, or weak participant field candidate; role remains unknown",
                action.name
            ),
        }));
    }
    if selected.is_some_and(|candidate| candidate.source == "field-name") {
        warnings.push(serde_json::json!({
            "code": "PG-ROLE-WEAK-FIELD",
            "severity": "warning",
            "action_name": &action.name,
            "message": format!(
                "action '{}' role '{}' comes only from a field name and does not prove signer or authorization authority",
                action.name,
                selected.map_or("unknown", |candidate| candidate.role.as_str())
            ),
        }));
    }
    if conflict {
        let sources = action
            .protocol_role_candidates
            .iter()
            .map(|candidate| format!("{}@{}", candidate.role, candidate.source))
            .collect::<Vec<_>>();
        warnings.push(serde_json::json!({
            "code": "PG-ROLE-CONFLICT",
            "severity": "warning",
            "action_name": &action.name,
            "message": format!(
                "action '{}' has conflicting role candidates [{}]; selected '{}@{}' by canonical source precedence",
                action.name,
                sources.join(", "),
                selected.map_or("unknown", |candidate| candidate.role.as_str()),
                selected.map_or("none", |candidate| candidate.source.as_str())
            ),
        }));
    }

    let status = if selected.is_none() {
        "missing"
    } else if conflict {
        "conflicting"
    } else if selected.is_some_and(|candidate| candidate.source == "field-name") {
        "weak-field-inference"
    } else {
        "attributed"
    };

    serde_json::json!({
        "role": selected.map(|candidate| candidate.role.as_str()),
        "role_source": selected.map(|candidate| candidate.source.as_str()),
        "role_strength": selected.map(|candidate| candidate.strength.as_str()),
        "role_status": status,
        "role_source_used": selected,
        "role_candidates": &action.protocol_role_candidates,
        "role_conflict": conflict,
        "role_warnings": warnings,
    })
}

fn protocol_graph_role_lints(edges: &[serde_json::Value]) -> Vec<serde_json::Value> {
    let mut lints = BTreeMap::new();
    for edge in edges {
        let Some(warnings) = edge.get("role_warnings").and_then(serde_json::Value::as_array) else {
            continue;
        };
        for warning in warnings {
            let action = warning.get("action_name").and_then(serde_json::Value::as_str).unwrap_or_default();
            let code = warning.get("code").and_then(serde_json::Value::as_str).unwrap_or_default();
            let message = warning.get("message").and_then(serde_json::Value::as_str).unwrap_or_default();
            lints.entry(format!("{action}\0{code}\0{message}")).or_insert_with(|| warning.clone());
        }
    }
    lints.into_values().collect()
}

fn protocol_graph_type_pattern_vertices(action: &crate::ActionMetadata, type_names: &BTreeSet<String>) -> (Vec<String>, Vec<String>) {
    let param_types = action
        .params
        .iter()
        .filter_map(|param| protocol_graph_base_type(&param.ty, type_names).map(|ty| (param.name.as_str(), ty)))
        .collect::<BTreeMap<_, _>>();
    let mut sources = BTreeSet::new();
    let mut targets = BTreeSet::new();

    for pattern in &action.consume_set {
        if let Some(ty) = param_types.get(pattern.binding.as_str()) {
            sources.insert(protocol_graph_type_vertex_id(ty));
        }
    }
    for pattern in &action.mutate_set {
        sources.insert(protocol_graph_type_vertex_id(&pattern.ty));
        targets.insert(protocol_graph_type_vertex_id(&pattern.ty));
    }
    for pattern in &action.create_set {
        targets.insert(protocol_graph_type_vertex_id(&pattern.ty));
    }

    if sources.is_empty() {
        sources.insert("transaction:start".to_string());
    }
    if targets.is_empty() {
        targets.insert("transaction:end".to_string());
    }

    (sources.into_iter().collect(), targets.into_iter().collect())
}

fn protocol_graph_base_type(ty: &str, type_names: &BTreeSet<String>) -> Option<String> {
    let trimmed = ty.trim().trim_start_matches('&').trim();
    let trimmed = trimmed.strip_prefix("mut ").unwrap_or(trimmed).trim();
    let base = trimmed.split('<').next().unwrap_or(trimmed).trim();
    type_names.contains(base).then(|| base.to_string())
}

fn protocol_graph_type_vertex_id(type_name: &str) -> String {
    type_name.to_string()
}

fn protocol_graph_state_vertex_id(type_name: &str, state: &str) -> String {
    format!("{}:{}", type_name, state)
}

fn protocol_graph_has_cycle(edges: &[serde_json::Value]) -> bool {
    let mut adjacency = BTreeMap::<String, Vec<String>>::new();
    for edge in edges {
        let Some(source) = edge.get("source_vertex").and_then(serde_json::Value::as_str) else {
            continue;
        };
        let Some(target) = edge.get("target_vertex").and_then(serde_json::Value::as_str) else {
            continue;
        };
        adjacency.entry(source.to_string()).or_default().push(target.to_string());
    }

    let mut visiting = BTreeSet::new();
    let mut visited = BTreeSet::new();
    for node in adjacency.keys() {
        if protocol_graph_cycle_visit(node, &adjacency, &mut visiting, &mut visited) {
            return true;
        }
    }
    false
}

fn protocol_graph_cycle_visit(
    node: &str,
    adjacency: &BTreeMap<String, Vec<String>>,
    visiting: &mut BTreeSet<String>,
    visited: &mut BTreeSet<String>,
) -> bool {
    if visited.contains(node) {
        return false;
    }
    if !visiting.insert(node.to_string()) {
        return true;
    }
    if let Some(targets) = adjacency.get(node) {
        for target in targets {
            if protocol_graph_cycle_visit(target, adjacency, visiting, visited) {
                return true;
            }
        }
    }
    visiting.remove(node);
    visited.insert(node.to_string());
    false
}

pub(super) fn protocol_graph_mermaid(graph: &serde_json::Value) -> String {
    let mut output = String::from("flowchart LR\n");
    let mut ids = BTreeMap::new();
    if let Some(vertices) = graph.get("vertices").and_then(serde_json::Value::as_array) {
        for (index, vertex) in vertices.iter().enumerate() {
            let id = vertex.get("id").and_then(serde_json::Value::as_str).unwrap_or("vertex");
            let label = vertex.get("label").and_then(serde_json::Value::as_str).unwrap_or(id);
            let mermaid_id = format!("v{}", index);
            ids.insert(id.to_string(), mermaid_id.clone());
            let initial = vertex.get("initial").and_then(serde_json::Value::as_bool).unwrap_or(false);
            let terminal = vertex.get("terminal").and_then(serde_json::Value::as_bool).unwrap_or(false);
            let escaped = protocol_graph_mermaid_escape(label);
            if initial {
                output.push_str(&format!("  {}((\"{}\"))\n", mermaid_id, escaped));
            } else if terminal {
                output.push_str(&format!("  {}[[\"{}\"]]\n", mermaid_id, escaped));
            } else {
                output.push_str(&format!("  {}[\"{}\"]\n", mermaid_id, escaped));
            }
        }
    }
    if let Some(edges) = graph.get("edges").and_then(serde_json::Value::as_array) {
        for edge in edges {
            let Some(source) = edge.get("source_vertex").and_then(serde_json::Value::as_str).and_then(|id| ids.get(id)) else {
                continue;
            };
            let Some(target) = edge.get("target_vertex").and_then(serde_json::Value::as_str).and_then(|id| ids.get(id)) else {
                continue;
            };
            let action = edge.get("action_name").and_then(serde_json::Value::as_str).unwrap_or("action");
            let label = match (
                edge.get("role").and_then(serde_json::Value::as_str),
                edge.get("role_source").and_then(serde_json::Value::as_str),
            ) {
                (Some(role), Some(source)) => format!("{action} · {role} via {source}"),
                _ => format!("{action} · role unknown"),
            };
            output.push_str(&format!("  {} -->|{}| {}\n", source, protocol_graph_mermaid_escape(&label), target));
        }
    }
    output
}

fn protocol_graph_mermaid_escape(label: &str) -> String {
    label.replace('\\', "\\\\").replace('"', "\\\"").replace('|', " ")
}
