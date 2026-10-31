//! Native CLI JSON input and machine/human result presentation.
use crate::error::{CompileError, Result};
use std::path::Path;

#[derive(Debug, Clone, Copy)]
pub(super) enum CliDiagnosticCode {
    GeneratedLockfileMissing,
}

impl CliDiagnosticCode {
    pub(super) fn error(self) -> CompileError {
        match self {
            Self::GeneratedLockfileMissing => CompileError::without_span(
                "Cell.lock is missing after lock generation; check concurrent filesystem changes and rerun cellc lock",
            )
            .with_code("CLI0003")
            .with_category(crate::error::CompileErrorCategory::Io),
        }
    }
}

#[derive(Debug)]
pub(super) struct CommandOutcome {
    pub(super) machine: serde_json::Value,
    pub(super) human_lines: Vec<String>,
}

impl CommandOutcome {
    pub(super) fn emit(self, json: bool) -> Result<()> {
        if json {
            print_json(&self.machine)
        } else {
            for line in self.human_lines {
                println!("{}", line);
            }
            Ok(())
        }
    }

    pub(super) fn failure(self, message: impl Into<String>) -> CompileError {
        CompileError::without_span(message).with_details(self.machine)
    }
}

pub(super) fn read_json_value(path: &Path) -> Result<serde_json::Value> {
    let bytes = std::fs::read(path).map_err(|error| {
        crate::error::CompileError::without_span(format!("failed to read JSON '{}': {}", path.display(), error))
            .with_category(crate::error::CompileErrorCategory::Io)
            .with_source(error)
    })?;
    serde_json::from_slice(&bytes)
        .map_err(|error| crate::error::CompileError::without_span(format!("failed to parse JSON '{}': {}", path.display(), error)))
}

pub(super) fn print_json(value: &serde_json::Value) -> Result<()> {
    let json = serde_json::to_string_pretty(value).map_err(|error| {
        crate::error::CompileError::without_span(format!("failed to serialize JSON: {}", error))
            .with_category(crate::error::CompileErrorCategory::Internal)
            .with_source(error)
    })?;
    println!("{}", json);
    Ok(())
}

pub(super) fn json_output(matches: &clap::ArgMatches) -> bool {
    matches.get_flag("json") || matches.get_one::<String>("message-format").is_some_and(|format| format == "json")
}
