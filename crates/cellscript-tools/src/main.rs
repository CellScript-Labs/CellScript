//! Phase-one Rust ports for low-risk CellScript repository tooling.
//!
//! The dev and CI gates compare these commands with their Python counterparts
//! through `scripts/dev/dual_run_tools.sh`. Python remains authoritative for
//! tools that have not completed byte-for-byte migration.

use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand};

mod shared;
mod skill_pack;
mod tooling_release;

#[derive(Debug, Parser)]
#[command(name = "cellscript-tools", version, about = "Rust ports of low-risk CellScript repository tooling")]
struct Cli {
    /// Override repository-root autodetection.
    #[arg(long, global = true, value_name = "PATH")]
    root: Option<PathBuf>,

    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Port of `scripts/validate_cellscript_tooling_release.py`.
    ValidateToolingRelease,
    /// Port of `scripts/check_cellscript_skill_pack.py`.
    CheckSkillPack,
}

fn failure(error: anyhow::Error) -> ExitCode {
    eprintln!("{error:#}");
    ExitCode::FAILURE
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let root = match shared::resolve_repo_root(cli.root.as_deref()) {
        Ok(root) => root,
        Err(error) => return failure(error),
    };

    match cli.command {
        Command::ValidateToolingRelease => match tooling_release::run(&root) {
            Ok(()) => ExitCode::SUCCESS,
            Err(error) => failure(error),
        },
        Command::CheckSkillPack => match skill_pack::run(&root) {
            Ok(0) => ExitCode::SUCCESS,
            Ok(_) => ExitCode::FAILURE,
            Err(error) => failure(error),
        },
    }
}
