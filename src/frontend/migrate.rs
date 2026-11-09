use super::parse;
use crate::edition::CellScriptEdition;
use crate::error::{CompileError, Result, Span};
use crate::lexer;
use crate::lexer::token::{Token, TokenKind};
use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TemporalMigrationCandidate {
    pub schema: String,
    pub source_edition: String,
    pub migration: String,
    pub replacements: usize,
    pub source: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LegacyTemporalApi {
    CurrentTimepoint,
    HeaderEpochNumber,
    HeaderEpochStartBlockNumber,
    HeaderEpochLength,
    InputSince,
    InputSinceAt,
    SinceEpochAbsolute,
    SinceEpochRelative,
}

impl LegacyTemporalApi {
    fn from_qualified_name(namespace: &str, name: &str) -> Option<Self> {
        match (namespace, name) {
            ("env", "current_timepoint") => Some(Self::CurrentTimepoint),
            ("ckb", "header_epoch_number") => Some(Self::HeaderEpochNumber),
            ("ckb", "header_epoch_start_block_number") => Some(Self::HeaderEpochStartBlockNumber),
            ("ckb", "header_epoch_length") => Some(Self::HeaderEpochLength),
            ("ckb", "input_since") => Some(Self::InputSince),
            ("ckb", "input_since_at") => Some(Self::InputSinceAt),
            ("ckb", "since_epoch_absolute") => Some(Self::SinceEpochAbsolute),
            ("ckb", "since_epoch_relative") => Some(Self::SinceEpochRelative),
            _ => None,
        }
    }

    fn qualified_name(self) -> &'static str {
        match self {
            Self::CurrentTimepoint => "env::current_timepoint",
            Self::HeaderEpochNumber => "ckb::header_epoch_number",
            Self::HeaderEpochStartBlockNumber => "ckb::header_epoch_start_block_number",
            Self::HeaderEpochLength => "ckb::header_epoch_length",
            Self::InputSince => "ckb::input_since",
            Self::InputSinceAt => "ckb::input_since_at",
            Self::SinceEpochAbsolute => "ckb::since_epoch_absolute",
            Self::SinceEpochRelative => "ckb::since_epoch_relative",
        }
    }

    fn replacement_summary(self) -> &'static str {
        match self {
            Self::CurrentTimepoint | Self::HeaderEpochNumber => "ckb::epoch_number_to_u64(ckb::header_dep(0).epoch_number)",
            Self::HeaderEpochStartBlockNumber => "ckb::block_number_to_u64(ckb::header_dep(0).epoch_start_block_number)",
            Self::HeaderEpochLength => "ckb::epoch_length_to_u64(ckb::header_dep(0).epoch_length)",
            Self::InputSince => "ckb::input_since_raw()",
            Self::InputSinceAt => "ckb::since_to_raw((input).since)",
            Self::SinceEpochAbsolute => "ckb::since_to_raw(ckb::since_absolute_epoch(...))",
            Self::SinceEpochRelative => "ckb::since_to_raw(ckb::since_relative_epoch(...))",
        }
    }

    fn requires_no_arguments(self) -> bool {
        matches!(
            self,
            Self::CurrentTimepoint
                | Self::HeaderEpochNumber
                | Self::HeaderEpochStartBlockNumber
                | Self::HeaderEpochLength
                | Self::InputSince
        )
    }
}

#[derive(Debug, Clone, Copy)]
struct LegacyTemporalCall {
    api: LegacyTemporalApi,
    span: Span,
    start: usize,
    name_start: usize,
    name_end: usize,
    lparen_end: usize,
    rparen_start: usize,
    end: usize,
}

#[derive(Debug)]
struct SourceEdit {
    start: usize,
    end: usize,
    replacement: &'static str,
}

/// Rewrite the legacy raw-`u64` CKB temporal calls to explicit typed-domain
/// operations while preserving the surrounding raw result type. The edit is
/// lexical, so comments and formatting outside the qualified call names remain
/// byte-for-byte intact. It never changes a package manifest or source edition.
pub fn migrate_legacy_temporal_source(source: &str, edition: CellScriptEdition) -> Result<TemporalMigrationCandidate> {
    parse(source, edition)?;
    let calls = legacy_temporal_calls(source)?;
    let mut edits = Vec::new();
    for call in &calls {
        match call.api {
            LegacyTemporalApi::CurrentTimepoint | LegacyTemporalApi::HeaderEpochNumber => edits.push(SourceEdit {
                start: call.start,
                end: call.end,
                replacement: "ckb::epoch_number_to_u64(ckb::header_dep(0).epoch_number)",
            }),
            LegacyTemporalApi::HeaderEpochStartBlockNumber => edits.push(SourceEdit {
                start: call.start,
                end: call.end,
                replacement: "ckb::block_number_to_u64(ckb::header_dep(0).epoch_start_block_number)",
            }),
            LegacyTemporalApi::HeaderEpochLength => edits.push(SourceEdit {
                start: call.start,
                end: call.end,
                replacement: "ckb::epoch_length_to_u64(ckb::header_dep(0).epoch_length)",
            }),
            LegacyTemporalApi::InputSince => {
                edits.push(SourceEdit { start: call.start, end: call.end, replacement: "ckb::input_since_raw()" })
            }
            LegacyTemporalApi::InputSinceAt => {
                edits.push(SourceEdit { start: call.start, end: call.lparen_end, replacement: "ckb::since_to_raw((" });
                edits.push(SourceEdit { start: call.rparen_start, end: call.rparen_start, replacement: ").since" });
            }
            LegacyTemporalApi::SinceEpochAbsolute | LegacyTemporalApi::SinceEpochRelative => {
                edits.push(SourceEdit { start: call.start, end: call.start, replacement: "ckb::since_to_raw(" });
                edits.push(SourceEdit {
                    start: call.name_start,
                    end: call.name_end,
                    replacement: if call.api == LegacyTemporalApi::SinceEpochAbsolute {
                        "since_absolute_epoch"
                    } else {
                        "since_relative_epoch"
                    },
                });
                edits.push(SourceEdit { start: call.end, end: call.end, replacement: ")" });
            }
        }
    }
    edits.sort_by(|left, right| right.start.cmp(&left.start).then_with(|| right.end.cmp(&left.end)));
    let mut migrated = source.to_string();
    for edit in edits {
        migrated.replace_range(edit.start..edit.end, edit.replacement);
    }
    parse(&migrated, edition).map_err(|error| {
        CompileError::new(
            format!("generated temporal migration candidate failed its frontend contract: {}", error.message),
            error.span,
        )
    })?;
    Ok(TemporalMigrationCandidate {
        schema: "cellscript-temporal-source-migration-v1".to_string(),
        source_edition: edition.as_str().to_string(),
        migration: "legacy-raw-ckb-temporal-to-explicit-typed-v1".to_string(),
        replacements: calls.len(),
        source: migrated,
    })
}

/// Targeted warnings for every legacy temporal call that has a total,
/// raw-result-compatible typed-domain replacement.
pub fn legacy_temporal_migration_diagnostics(source: &str) -> Vec<CompileError> {
    let Ok(calls) = legacy_temporal_calls(source) else {
        return Vec::new();
    };
    calls
        .into_iter()
        .map(|call| {
            CompileError::warning(
                format!(
                    "legacy raw temporal API '{}' returns a raw u64; migrate mechanically to '{}'",
                    call.api.qualified_name(),
                    call.api.replacement_summary()
                ),
                call.span,
            )
            .with_code(crate::error::DiagnosticCode::W3012)
            .with_details(serde_json::json!({
                "legacy_api": call.api.qualified_name(),
                "replacement": call.api.replacement_summary(),
                "migration": "legacy-raw-ckb-temporal-to-explicit-typed-v1",
            }))
        })
        .collect()
}

fn legacy_temporal_calls(source: &str) -> Result<Vec<LegacyTemporalCall>> {
    let tokens = lexer::lex(source)?;
    let mut calls = Vec::new();
    let mut index = 0usize;
    while index + 3 < tokens.len() {
        let Some(namespace) = token_name(&tokens[index]) else {
            index += 1;
            continue;
        };
        if tokens[index + 1].kind != TokenKind::ColonColon || tokens[index + 3].kind != TokenKind::LParen {
            index += 1;
            continue;
        }
        let Some(name) = token_name(&tokens[index + 2]) else {
            index += 1;
            continue;
        };
        let Some(api) = LegacyTemporalApi::from_qualified_name(namespace, name) else {
            index += 1;
            continue;
        };
        let Some(rparen_index) = matching_rparen(&tokens, index + 3) else {
            index += 1;
            continue;
        };
        if api.requires_no_arguments() && tokens[index + 4..rparen_index].iter().any(|token| token.kind != TokenKind::Newline) {
            index += 1;
            continue;
        }
        let start = tokens[index].span.start;
        let end = tokens[rparen_index].span.end;
        calls.push(LegacyTemporalCall {
            api,
            span: Span::new(start, end, tokens[index].span.line, tokens[index].span.column),
            start,
            name_start: tokens[index + 2].span.start,
            name_end: tokens[index + 2].span.end,
            lparen_end: tokens[index + 3].span.end,
            rparen_start: tokens[rparen_index].span.start,
            end,
        });
        index += 1;
    }
    Ok(calls)
}

fn token_name(token: &Token) -> Option<&str> {
    match &token.kind {
        TokenKind::Identifier(name) => Some(name),
        TokenKind::Env => Some("env"),
        _ => None,
    }
}

fn matching_rparen(tokens: &[Token], lparen_index: usize) -> Option<usize> {
    let mut depth = 0usize;
    for (index, token) in tokens.iter().enumerate().skip(lparen_index) {
        match token.kind {
            TokenKind::LParen => depth += 1,
            TokenKind::RParen => {
                depth = depth.checked_sub(1)?;
                if depth == 0 {
                    return Some(index);
                }
            }
            _ => {}
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{compile, CompileOptions};

    #[test]
    fn temporal_migration_preserves_raw_results_comments_and_nested_calls() {
        let source = r#"module temporal;
resource Token has store { amount: u64, }
fn inspect() -> u64 {
    let input = ckb::input::<Token>(0);
    let epoch = env::current_timepoint();
    let header_epoch = ckb::header_epoch_number();
    let start = ckb::header_epoch_start_block_number();
    let length = ckb::header_epoch_length();
    let implicit_since = ckb::input_since();
    let observed = ckb::input_since_at(input /* keep */);
    let required = ckb::since_epoch_absolute(42, 3, 10);
    let nested = ckb::since_epoch_relative(ckb::header_epoch_number(), 1, 4);
    return epoch + header_epoch + start + length + implicit_since + observed + required + nested;
}
"#;
        let candidate = migrate_legacy_temporal_source(source, CellScriptEdition::Edition2027).unwrap();
        assert_eq!(candidate.replacements, 9);
        assert!(candidate.source.contains("input /* keep */).since"));
        assert!(candidate.source.contains("ckb::epoch_number_to_u64(ckb::header_dep(0).epoch_number)"));
        assert!(candidate.source.contains(
            "ckb::since_to_raw(ckb::since_relative_epoch(ckb::epoch_number_to_u64(ckb::header_dep(0).epoch_number), 1, 4))"
        ));
        let original = compile(
            source,
            CompileOptions {
                diagnostics: crate::CompilerDiagnostics::default().without(crate::OptimizationPass::DeadCode),
                ..Default::default()
            },
        )
        .unwrap();
        let migrated = compile(
            &candidate.source,
            CompileOptions {
                diagnostics: crate::CompilerDiagnostics::default().without(crate::OptimizationPass::DeadCode),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(original.metadata.functions[0].return_type, migrated.metadata.functions[0].return_type);
        assert!(legacy_temporal_migration_diagnostics(&candidate.source).is_empty());
        let diagnostics = legacy_temporal_migration_diagnostics(source);
        assert_eq!(diagnostics.len(), 9);
        assert!(diagnostics.iter().all(|diagnostic| diagnostic.code.as_deref() == Some("W3012")));
    }
}
