//! CLI module
//! Command-line interface and subcommand implementation

pub mod commands;
mod novaseal_certification;

use crate::error::Result;
use commands::{CliParser, CommandExecutor};
use std::io::IsTerminal;

pub fn no_color_env_set() -> bool {
    std::env::var_os("NO_COLOR").map(|value| !value.is_empty()).unwrap_or(false)
}

pub fn apply_color_policy(choice: Option<&str>) {
    match choice.unwrap_or("auto") {
        "always" => colored::control::set_override(true),
        "never" => colored::control::set_override(false),
        _ => {
            if no_color_env_set() || (!std::io::stdout().is_terminal() && !std::io::stderr().is_terminal()) {
                colored::control::set_override(false);
            } else {
                colored::control::unset_override();
            }
        }
    }
}

/// Run CLI
pub fn run() -> Result<()> {
    let cmd = CliParser::parse()?;
    CommandExecutor::execute(cmd)
}
