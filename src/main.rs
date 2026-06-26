use camino::Utf8Path;
use cellscript::error::CompileError;
use clap::Parser;
use colored::Colorize;
use std::path::Path;
use std::process;

use cellscript::{
    compile_path, compile_path_metadata_with_diagnostics, compile_path_with_entry_action, compile_path_with_entry_lock,
    default_metadata_path_for_artifact, default_output_path_for_input, resolve_input_path, CompileOptions,
};

#[derive(Parser, Debug)]
#[command(name = "cellc")]
#[command(about = "CellScript compiler for CKB blockchain")]
#[command(version = cellscript::VERSION)]
struct Cli {
    #[arg(value_name = "INPUT")]
    input: Option<String>,

    #[arg(short = 'O', long, default_value = "0")]
    opt: u8,

    #[arg(short, long, value_name = "FILE")]
    output: Option<String>,

    #[arg(short, long)]
    debug: bool,

    #[arg(short, long)]
    target: Option<String>,

    #[arg(long)]
    target_profile: Option<String>,

    #[arg(long, value_name = "VERSION", conflicts_with = "primitive_strict")]
    primitive_compat: Option<String>,

    #[arg(long, value_name = "VERSION", conflicts_with = "primitive_compat")]
    primitive_strict: Option<String>,

    #[arg(long, value_name = "ACTION")]
    entry_action: Option<String>,

    #[arg(long, value_name = "LOCK")]
    entry_lock: Option<String>,

    #[arg(long)]
    lex: bool,

    #[arg(long)]
    parse: bool,

    #[arg(short, long)]
    interactive: bool,

    #[arg(long)]
    gen_stdlib: bool,

    /// Start the language server (JSON-RPC over stdio).
    #[arg(long)]
    lsp: bool,
}

fn main() {
    // Start the LSP server before any CLI parsing side effects.
    if std::env::args().any(|arg| arg == "--lsp") {
        cellscript::lsp::server::run_lsp_server_blocking();
        return;
    }

    let mut raw_args = std::env::args();
    let _program = raw_args.next();
    if let Some(arg) = raw_args.next() {
        match arg.as_str() {
            "--help" | "-h" => {
                print_top_level_help();
                return;
            }
            "--explain" => {
                let Some(code) = raw_args.next() else {
                    eprintln!("{}: the argument '--explain <CODE>' requires a value", "error".red());
                    process::exit(2);
                };
                run_top_level_explain(code);
                return;
            }
            "--list" => {
                print_command_list();
                return;
            }
            _ if is_package_command(&arg) || arg == "help" => {
                if let Err(e) = cellscript::cli::run() {
                    print_cli_error(&e);
                    process::exit(1);
                }
                return;
            }
            _ if looks_like_unknown_command(&arg) => {
                print_unknown_command(&arg);
                process::exit(1);
            }
            _ => {}
        }
        if let Some(code) = arg.strip_prefix("--explain=") {
            run_top_level_explain(code.to_string());
            return;
        }
    }

    let cli = Cli::parse();

    env_logger::init();

    if cli.interactive {
        if let Err(e) = cellscript::repl::run_repl() {
            eprintln!("{}: {}", "REPL error".red(), e);
            process::exit(1);
        }
        return;
    }

    if cli.gen_stdlib {
        let target_profile = cli
            .target_profile
            .as_deref()
            .map(cellscript::TargetProfile::from_name)
            .transpose()
            .unwrap_or_else(|e| {
                print_cli_error(&e);
                process::exit(1);
            })
            .unwrap_or(cellscript::TargetProfile::Ckb);
        let asm = cellscript::stdlib::StdLib::generate_assembly_for_target_profile(target_profile);
        println!("{}", asm);
        return;
    }

    if cli.opt > 3 {
        eprintln!("{}: optimization level must be between 0 and 3", "error".red());
        process::exit(1);
    }

    let input_file = cli.input.unwrap_or_else(|| ".".to_string());
    let resolved_input = match resolve_input_path(Utf8Path::new(&input_file)) {
        Ok(path) => path,
        Err(e) => {
            print_cli_error(&e);
            process::exit(1);
        }
    };

    let source = match std::fs::read_to_string(&resolved_input) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("{}: failed to read '{}': {}", "error".red(), resolved_input, e);
            process::exit(1);
        }
    };

    if cli.lex {
        match cellscript::lexer::lex(&source) {
            Ok(tokens) => {
                println!("{}: found {} tokens", "success".green(), tokens.len());
                for token in tokens {
                    println!("  {:?}", token);
                }
            }
            Err(e) => {
                print_cli_error_with_source(&e, Some(&resolved_input), Some(&source));
                process::exit(1);
            }
        }
        return;
    }

    if cli.parse {
        let tokens = match cellscript::lexer::lex(&source) {
            Ok(t) => t,
            Err(e) => {
                print_cli_error_with_source(&e, Some(&resolved_input), Some(&source));
                process::exit(1);
            }
        };

        match cellscript::parser::parse_diagnostics(&tokens) {
            Ok(ast) => {
                println!("{}: parsed successfully", "success".green());
                println!("{:#?}", ast);
            }
            Err(diagnostics) => {
                let error = diagnostics_to_cli_error(diagnostics);
                print_cli_error_with_source(&error, Some(&resolved_input), Some(&source));
                process::exit(1);
            }
        }
        return;
    }

    let output = cli.output.clone();
    let options = CompileOptions {
        opt_level: cli.opt,
        output: output.clone(),
        debug: cli.debug,
        target: cli.target,
        target_profile: cli.target_profile,
        primitive_compat: resolve_primitive_compat(cli.primitive_compat, cli.primitive_strict),
    };

    if cli.entry_action.is_some() && cli.entry_lock.is_some() {
        eprintln!("{}: --entry-action and --entry-lock are mutually exclusive", "error".red());
        process::exit(1);
    }

    let diagnostics_options = options.clone();
    let compile_result = match (cli.entry_action, cli.entry_lock) {
        (Some(action), None) => compile_path_with_entry_action(Utf8Path::new(&input_file), options, action),
        (None, Some(lock)) => compile_path_with_entry_lock(Utf8Path::new(&input_file), options, lock),
        (None, None) => compile_path(Utf8Path::new(&input_file), options),
        (Some(_), Some(_)) => unreachable!("validated above"),
    };

    match compile_result {
        Ok(result) => {
            let output_path = output
                .as_deref()
                .map(Utf8Path::new)
                .map(|path| path.to_owned())
                .map(Ok)
                .unwrap_or_else(|| default_output_path_for_input(Utf8Path::new(&input_file), &resolved_input, result.artifact_format))
                .unwrap_or_else(|e| {
                    print_cli_error(&e);
                    process::exit(1);
                });

            if let Err(e) = result.write_to_path(&output_path) {
                print_cli_error(&e);
                process::exit(1);
            }
            let metadata_path = default_metadata_path_for_artifact(&output_path);
            if let Err(e) = result.write_metadata_to_path(&metadata_path) {
                print_cli_error(&e);
                process::exit(1);
            }

            println!("{}: compiled successfully", "success".green());
            println!("  Artifact format: {}", result.artifact_format.display_name());
            println!("  Target profile: {}", result.metadata.target_profile.name);
            println!("  Artifact hash: {:x?}", result.artifact_hash);
            println!("  Output: {}", output_path);
            println!("  Metadata: {}", metadata_path);
        }
        Err(e) => {
            let report = compile_path_metadata_with_diagnostics(Utf8Path::new(&input_file), diagnostics_options);
            if report.diagnostics.is_empty() {
                print_cli_error_with_source(&e, Some(&resolved_input), Some(&source));
            } else {
                let error = diagnostics_to_cli_error(report.diagnostics);
                print_cli_error_with_source(&error, Some(&resolved_input), Some(&source));
            }
            process::exit(1);
        }
    }
}

fn resolve_primitive_compat(compat: Option<String>, strict: Option<String>) -> Option<String> {
    if strict.is_some() {
        strict
    } else {
        compat
    }
}

fn print_cli_error(error: &CompileError) {
    print_cli_error_with_source(error, None, None);
}

fn diagnostics_to_cli_error(mut diagnostics: Vec<CompileError>) -> CompileError {
    match diagnostics.len() {
        0 => CompileError::without_span("compilation failed"),
        1 => diagnostics.remove(0),
        len => CompileError::without_span(format!("aborting due to {} diagnostics", len)).with_related(diagnostics),
    }
}

fn print_cli_error_with_source(error: &CompileError, fallback_file: Option<&Utf8Path>, fallback_source: Option<&str>) {
    if !error.related.is_empty() {
        for diagnostic in &error.related {
            print_single_cli_error(diagnostic, fallback_file, fallback_source);
        }
        let error_count =
            error.related.iter().filter(|diagnostic| diagnostic.severity == cellscript::error::DiagnosticSeverity::Error).count();
        let warning_count = error.related.len().saturating_sub(error_count);
        let noun = if error.related.len() == 1 { "diagnostic" } else { "diagnostics" };
        if warning_count > 0 {
            eprintln!("{}: aborting due to {} error(s) and {} warning(s)", "error".red(), error_count, warning_count);
        } else {
            eprintln!("{}: aborting due to {} {}", "error".red(), error.related.len(), noun);
        }
        return;
    }

    print_single_cli_error(error, fallback_file, fallback_source);
}

fn print_single_cli_error(error: &CompileError, fallback_file: Option<&Utf8Path>, fallback_source: Option<&str>) {
    let runtime_info = cellscript::runtime_errors::runtime_error_info_for_diagnostic_message(&error.message);
    let label = diagnostic_label(error, runtime_info.as_ref());
    if let Some((file, source)) = diagnostic_source(error, fallback_file, fallback_source) {
        eprintln!("{}: {}", colour_diagnostic_label(&label, error), error.message);
        print_source_snippet(file, &source, error);
    } else if error.span.line == 0 {
        eprintln!("{}: {}", colour_diagnostic_label(&label, error), error.message);
    } else {
        eprintln!("{}: {}", colour_diagnostic_label(&label, error), error);
    }

    if let Some(info) = cellscript::runtime_errors::runtime_error_info_for_diagnostic_message(&error.message) {
        eprintln!("  {}: run `cellc explain E{:04}` for {}", "help".cyan(), info.code, info.name);
    }
    print_followup_hints(error);
}

fn run_top_level_explain(code: String) {
    let command = cellscript::cli::commands::Command::Explain(cellscript::cli::commands::ExplainArgs { code, json: false });
    if let Err(error) = cellscript::cli::commands::CommandExecutor::execute(command) {
        print_cli_error(&error);
        process::exit(1);
    }
}

fn diagnostic_label(error: &CompileError, runtime_info: Option<&cellscript::runtime_errors::CellScriptRuntimeErrorInfo>) -> String {
    if let Some(info) = runtime_info {
        format!("error[E{:04}]", info.code)
    } else if let Some(code) = &error.code {
        format!("{}[{}]", error.severity.label(), code)
    } else {
        error.severity.label().to_string()
    }
}

fn colour_diagnostic_label(label: &str, error: &CompileError) -> colored::ColoredString {
    match error.severity {
        cellscript::error::DiagnosticSeverity::Warning => label.yellow(),
        cellscript::error::DiagnosticSeverity::Error => label.red(),
    }
}

fn diagnostic_source(
    error: &CompileError,
    fallback_file: Option<&Utf8Path>,
    fallback_source: Option<&str>,
) -> Option<(String, String)> {
    if error.span.line == 0 {
        return None;
    }

    let file = error.file.as_deref().or(fallback_file)?;
    if Some(file) == fallback_file {
        if let Some(source) = fallback_source {
            return Some((file.to_string(), source.to_string()));
        }
    }

    std::fs::read_to_string(file.as_std_path()).ok().map(|source| (file.to_string(), source))
}

fn print_source_snippet(file: String, source: &str, error: &CompileError) {
    let line_number = error.span.line;
    let line_text = source.lines().nth(line_number.saturating_sub(1)).unwrap_or("");
    let column = error.span.column.max(1);
    let line_width = line_number.to_string().len();
    let line_char_count = line_text.chars().count();
    let underline_offset = column.saturating_sub(1).min(line_char_count);
    let span_width = error.span.end.saturating_sub(error.span.start).max(1);
    let remaining_width = line_char_count.saturating_sub(underline_offset).max(1);
    let underline_width = span_width.min(remaining_width).max(1);
    let underline = format!("{}{}", " ".repeat(underline_offset), "^".repeat(underline_width));

    eprintln!(" {} {}:{}:{}", "-->".blue(), file, line_number, column);
    eprintln!("{:>width$} |", "", width = line_width);
    eprintln!("{:>width$} | {}", line_number, line_text, width = line_width);
    eprintln!("{:>width$} | {} {}", "", underline.red(), error.message, width = line_width);
}

fn is_package_command(arg: &str) -> bool {
    cellc_cli_command().get_subcommands().any(|command| command.get_name() == arg)
}

fn looks_like_unknown_command(arg: &str) -> bool {
    if arg.starts_with('-') || arg == "." || arg == ".." {
        return false;
    }
    if arg.contains('/') || arg.contains('\\') || arg.ends_with(".cell") || arg == "Cell.toml" {
        return false;
    }
    if arg.contains('.') || Path::new(arg).exists() {
        return false;
    }
    true
}

fn print_unknown_command(arg: &str) {
    eprintln!("{}: no such command or input: `{}`", "error".red(), arg);
    if let Some(suggestion) = closest_command(arg) {
        eprintln!("  {}: a command with a similar name exists: `{}`", "help".cyan(), suggestion);
    }
    eprintln!("  {}: run `cellc --help` to view commands and direct source mode", "help".cyan());
    eprintln!("  {}: pass a .cell file, package directory, or Cell.toml to compile directly", "help".cyan());
}

fn print_followup_hints(error: &CompileError) {
    let message = error.message.as_str();
    if message.contains("Cell.toml not found") {
        eprintln!("  {}: run `cellc init` to create a package in this directory", "help".cyan());
        eprintln!("  {}: pass a .cell file, package directory, or Cell.toml to compile directly", "help".cyan());
    } else if message.starts_with("unsupported input") {
        eprintln!("  {}: pass a .cell file, package directory, or Cell.toml", "help".cyan());
        eprintln!("  {}: run `cellc --help` to view direct source mode and package commands", "help".cyan());
    } else if message.starts_with("input file ") && message.contains(" does not exist") {
        eprintln!("  {}: check the path, or run `cellc init` to create a package", "help".cyan());
    }
}

fn closest_command(input: &str) -> Option<String> {
    cellc_cli_command()
        .get_subcommands()
        .map(|command| command.get_name().to_string())
        .map(|command| {
            let distance = edit_distance(input, &command);
            (command, distance)
        })
        .filter(|(command, distance)| *distance <= 3 || command.starts_with(input) || input.starts_with(command))
        .min_by_key(|(_, distance)| *distance)
        .map(|(command, _)| command)
}

fn edit_distance(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let mut previous: Vec<usize> = (0..=b.len()).collect();
    let mut current = vec![0; b.len() + 1];

    for (i, a_char) in a.iter().enumerate() {
        current[0] = i + 1;
        for (j, b_char) in b.iter().enumerate() {
            let substitution = previous[j] + usize::from(a_char != b_char);
            let insertion = current[j] + 1;
            let deletion = previous[j + 1] + 1;
            current[j + 1] = substitution.min(insertion).min(deletion);
        }
        std::mem::swap(&mut previous, &mut current);
    }

    previous[b.len()]
}

fn print_top_level_help() {
    println!("CellScript compiler and package manager for CKB blockchain\n");
    println!("Usage:");
    println!("  cellc [OPTIONS] [INPUT]");
    println!("  cellc <COMMAND> [OPTIONS]\n");
    println!("Direct source mode:");
    println!("  cellc examples/token.cell --target riscv64-elf --target-profile ckb -o target/token.elf");
    println!("  cellc . --target riscv64-asm --target-profile ckb\n");
    println!("Common commands:");
    for command in [
        "build",
        "check",
        "metadata",
        "verify-artifact",
        "action",
        "gen-builder",
        "validate-tx",
        "deploy-plan",
        "registry",
        "certify",
        "publish",
        "install",
        "fmt",
        "init",
    ] {
        if let Some(about) = package_command_about(command) {
            println!("  {:<18} {}", command, about);
        }
    }
    println!("\nDirect options:");
    println!("  -O, --opt <OPT>                  Optimization level 0..3 [default: 0]");
    println!("  -o, --output <FILE>              Write artifact to FILE");
    println!("  -d, --debug                      Include debug metadata where supported");
    println!("  -t, --target <TARGET>            Target: riscv64-asm or riscv64-elf");
    println!("      --target-profile <PROFILE>   Target profile: ckb");
    println!("      --entry-action <ACTION>      Compile one action as entrypoint");
    println!("      --entry-lock <LOCK>          Compile one lock as entrypoint");
    println!("      --primitive-compat <VERSION> Accept older primitive syntax with hints");
    println!("      --primitive-strict <VERSION> Reject legacy primitive syntax");
    println!("      --lex / --parse              Stop after lexing or parsing");
    println!("      --explain <CODE>             Explain a CellScript runtime error code");
    println!("  -i, --interactive                Start the REPL");
    println!("      --gen-stdlib                 Print generated standard library assembly");
    println!("      --lsp                        Start the language server over stdio");
    println!("  -V, --version                    Print version\n");
    println!("Run `cellc <command> --help` for command-specific options.");
    println!("Run `cellc --list` to see every command.");
}

fn print_command_list() {
    println!("Installed cellc commands:\n");
    for command in cellc_cli_command().get_subcommands() {
        let about = command.get_about().map(|about| about.to_string()).unwrap_or_default();
        println!("  {:<22} {}", command.get_name(), about);
    }
}

fn package_command_about(command_name: &str) -> Option<String> {
    cellc_cli_command()
        .get_subcommands()
        .find(|command| command.get_name() == command_name)
        .and_then(|command| command.get_about().map(|about| about.to_string()))
}

fn cellc_cli_command() -> clap::Command {
    cellscript::cli::commands::CliParser::command()
}
