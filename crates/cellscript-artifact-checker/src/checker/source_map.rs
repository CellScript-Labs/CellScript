//! Source-map identity, range coverage and confined source-path validation.
//! This layer validates bound records without loading compiler source phases.

use super::{map_elf_error, CheckerError, CheckerRejectionCode};
use crate::elf::ParsedElf;
use crate::schema::{SourceArtifactMap, VerifiedLoweringRecord, SOURCE_MAP_SCHEMA, SOURCE_MAP_VERSION};
use std::collections::{BTreeMap, BTreeSet};

pub(super) fn validate_source_map(
    source_map: &SourceArtifactMap,
    record: &VerifiedLoweringRecord,
    artifact: &[u8],
    elf: &ParsedElf,
) -> Result<(), CheckerError> {
    if source_map.schema != SOURCE_MAP_SCHEMA
        || source_map.version != SOURCE_MAP_VERSION
        || source_map.module != record.module
        || source_map.text_range != record.text_range
        || source_map.source_digest != record.source_content_hash
        || source_map.coverage_claim.source_semantic_equivalence
        || !source_map.coverage_claim.mapped_instruction_ranges_only
    {
        return Err(CheckerError::new(
            CheckerRejectionCode::V2416SourceMapInvalid,
            "source map schema, identity, or bounded claim is invalid",
        ));
    }
    let blocks = record.blocks.iter().map(|block| (block.id.as_str(), block)).collect::<BTreeMap<_, _>>();
    let entries = record.entries.iter().map(|entry| entry.id.as_str()).collect::<BTreeSet<_>>();
    let mut previous_end = None;
    let mut mapped_ranges = Vec::new();
    for interval in &source_map.intervals {
        if !safe_source_path(&interval.source_path)
            || interval.source_start > interval.source_end
            || interval.machine_range.is_empty()
            || interval.machine_range.start % 4 != 0
            || interval.machine_range.end % 4 != 0
            || previous_end.is_some_and(|end| interval.machine_range.start < end)
        {
            return Err(CheckerError::new(
                CheckerRejectionCode::V2416SourceMapInvalid,
                format!("source-map interval for block '{}' overlaps, escapes, or is malformed", interval.block_id),
            ));
        }
        let Some(block) = blocks.get(interval.block_id.as_str()) else {
            return Err(CheckerError::new(
                CheckerRejectionCode::V2416SourceMapInvalid,
                format!("source-map interval references missing block '{}'", interval.block_id),
            ));
        };
        if interval.entry_id != block.owner_entry
            || !entries.contains(interval.entry_id.as_str())
            || !block.range.contains_range(interval.machine_range)
            || interval.lowering_block_id != block.lowering_block_id
            || interval.proof_ids.iter().any(|proof| !block.proof_ids.contains(proof))
        {
            return Err(CheckerError::new(
                CheckerRejectionCode::V2416SourceMapInvalid,
                format!("source-map interval for '{}' disagrees with its lowering block", interval.block_id),
            ));
        }
        elf.bytes_for_range(artifact, interval.machine_range).map_err(map_elf_error)?;
        previous_end = Some(interval.machine_range.end);
        mapped_ranges.push(interval.machine_range);
    }
    let foundation = &record.typed_semantics.foundation;
    let mut semantic_ids = foundation.provenance.nodes.iter().map(|node| node.id.as_str()).collect::<BTreeSet<_>>();
    semantic_ids.insert(foundation.entry_contract.semantic_node_id.as_str());
    semantic_ids.extend(foundation.roles.iter().map(|role| role.semantic_node_id.as_str()));
    semantic_ids.extend(foundation.dispositions.iter().map(|item| item.semantic_node_id.as_str()));
    semantic_ids.extend(foundation.claims.iter().map(|claim| claim.semantic_node_id.as_str()));
    semantic_ids.extend(foundation.legacy_nodes.iter().map(|legacy| legacy.semantic_node_id.as_str()));
    if source_map.semantic_mappings.len() > 262_144
        || source_map.semantic_mappings.windows(2).any(|pair| {
            (&pair[0].semantic_node_id, &pair[0].source_path, pair[0].source_start, pair[0].source_end)
                >= (&pair[1].semantic_node_id, &pair[1].source_path, pair[1].source_start, pair[1].source_end)
        })
    {
        return Err(CheckerError::new(
            CheckerRejectionCode::V2416SourceMapInvalid,
            "semantic source mappings exceed their bound or are not canonical",
        ));
    }
    let mut mapped_semantic_ids = BTreeSet::new();
    for mapping in &source_map.semantic_mappings {
        if !semantic_ids.contains(mapping.semantic_node_id.as_str())
            || !safe_source_path(&mapping.source_path)
            || mapping.source_start > mapping.source_end
        {
            return Err(CheckerError::new(
                CheckerRejectionCode::V2416SourceMapInvalid,
                format!("semantic source mapping '{}' is malformed or references an unknown node", mapping.semantic_node_id),
            ));
        }
        mapped_semantic_ids.insert(mapping.semantic_node_id.as_str());
    }
    if mapped_semantic_ids != semantic_ids {
        return Err(CheckerError::new(
            CheckerRejectionCode::V2416SourceMapInvalid,
            "semantic source mappings do not cover every semantic node",
        ));
    }
    if source_map.coverage_claim.complete_text_coverage {
        let mut expected = record.text_range.start;
        for range in mapped_ranges {
            if range.start != expected {
                return Err(CheckerError::new(
                    CheckerRejectionCode::V2416SourceMapInvalid,
                    "source map claims complete text coverage but contains a gap",
                ));
            }
            expected = range.end;
        }
        if expected != record.text_range.end {
            return Err(CheckerError::new(
                CheckerRejectionCode::V2416SourceMapInvalid,
                "source map claims complete text coverage but does not reach text end",
            ));
        }
    }
    Ok(())
}

pub(super) fn safe_source_path(path: &str) -> bool {
    if path == "<memory>" {
        return true;
    }
    if path.is_empty() || path.starts_with('/') || path.starts_with('\\') || path.contains('\\') {
        return false;
    }
    if path.len() >= 2 && path.as_bytes()[1] == b':' {
        return false;
    }
    path.split('/').all(|component| !matches!(component, "" | "." | ".."))
}
