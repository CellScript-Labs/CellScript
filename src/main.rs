use camino::Utf8Path;
use cellscript::error::{CompileError, CompileErrorCategory};
use clap::{Parser, ValueEnum};
use colored::Colorize;
use std::error::Error as _;
use std::path::Path;
use std::process;
use unicode_width::UnicodeWidthStr;

use cellscript::{
    compile_path, compile_path_metadata_with_diagnostics, compile_path_with_entry_action, compile_path_with_entry_lock,
    default_metadata_path_for_artifact, default_output_path_for_input, resolve_input_path, CompileOptions,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
enum MessageFormat {
    Human,
    Json,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
enum ColorChoice {
    Auto,
    Always,
    Never,
}

impl ColorChoice {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Always => "always",
            Self::Never => "never",
        }
    }
}

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

    #[arg(long, value_enum, default_value = "human")]
    message_format: MessageFormat,

    #[arg(long, value_enum, default_value = "auto")]
    color: ColorChoice,

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
    let requested_color = requested_color();
    cellscript::cli::apply_color_policy(requested_color.as_deref());
    let requested_message_format = requested_message_format();

    // Start the LSP server before any CLI parsing side effects.
    if std::env::args().any(|arg| arg == "--lsp") {
        cellscript::lsp::server::run_lsp_server_blocking();
        return;
    }

    let raw_args = std::env::args().skip(1).collect::<Vec<_>>();
    if let Some((arg_index, arg)) = routing_argument(&raw_args) {
        match arg {
            "--help" | "-h" => {
                print_top_level_help();
                return;
            }
            "--explain" => {
                let Some(code) = raw_args.get(arg_index + 1) else {
                    eprintln!("{}: the argument '--explain <CODE>' requires a value", "error".red());
                    process::exit(2);
                };
                run_top_level_explain(code.clone());
                return;
            }
            "--list" => {
                print_command_list();
                return;
            }
            _ if is_package_command(arg) || arg == "help" => {
                if let Err(e) = cellscript::cli::run() {
                    terminate_cli_error(&e, requested_message_format, None, None);
                }
                return;
            }
            _ if looks_like_unknown_command(arg) => {
                print_unknown_command(arg);
                process::exit(CompileErrorCategory::Usage.exit_code());
            }
            _ => {}
        }
        if let Some(code) = arg.strip_prefix("--explain=") {
            run_top_level_explain(code.to_string());
            return;
        }
    }

    let cli = Cli::try_parse().unwrap_or_else(|error| {
        if matches!(error.kind(), clap::error::ErrorKind::DisplayHelp | clap::error::ErrorKind::DisplayVersion) {
            if let Err(io_error) = error.print() {
                terminate_cli_error(&CompileError::from(io_error), requested_message_format, None, None);
            }
            process::exit(0);
        }
        let error = CompileError::without_span(error.to_string()).with_code("CLI0001").with_category(CompileErrorCategory::Usage);
        terminate_cli_error(&error, requested_message_format, None, None)
    });
    let message_format = cli.message_format;
    cellscript::cli::apply_color_policy(Some(cli.color.as_str()));

    env_logger::init();

    if cli.interactive {
        if let Err(e) = cellscript::repl::run_repl() {
            terminate_cli_error(&CompileError::from(e), message_format, None, None);
        }
        return;
    }

    if cli.gen_stdlib {
        let target_profile = cli
            .target_profile
            .as_deref()
            .map(cellscript::TargetProfile::from_name)
            .transpose()
            .unwrap_or_else(|e| terminate_cli_error(&e, message_format, None, None))
            .unwrap_or(cellscript::TargetProfile::Ckb);
        let asm = cellscript::stdlib::StdLib::generate_assembly_for_target_profile(target_profile);
        println!("{}", asm);
        return;
    }

    if cli.opt > 3 {
        let error =
            CompileError::without_span("optimization level must be between 0 and 3").with_category(CompileErrorCategory::Usage);
        terminate_cli_error(&error, message_format, None, None);
    }

    let input_file = cli.input.unwrap_or_else(|| ".".to_string());
    let resolved_input = match resolve_input_path(Utf8Path::new(&input_file)) {
        Ok(path) => path,
        Err(e) => {
            terminate_cli_error(&e, message_format, None, None);
        }
    };

    let source = match std::fs::read_to_string(&resolved_input) {
        Ok(s) => s,
        Err(e) => {
            let error = CompileError::without_span(format!("failed to read '{}': {}", resolved_input, e))
                .with_category(CompileErrorCategory::Io)
                .with_file(resolved_input.clone())
                .with_source(e);
            terminate_cli_error(&error, message_format, None, None);
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
                terminate_cli_error(&e, message_format, Some(&resolved_input), Some(&source));
            }
        }
        return;
    }

    if cli.parse {
        let tokens = match cellscript::lexer::lex(&source) {
            Ok(t) => t,
            Err(e) => {
                terminate_cli_error(&e, message_format, Some(&resolved_input), Some(&source));
            }
        };

        match cellscript::parser::parse_diagnostics(&tokens) {
            Ok(ast) => {
                println!("{}: parsed successfully", "success".green());
                println!("{:#?}", ast);
            }
            Err(diagnostics) => {
                let error = diagnostics_to_cli_error(diagnostics);
                terminate_cli_error(&error, message_format, Some(&resolved_input), Some(&source));
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
        let error = CompileError::without_span("--entry-action and --entry-lock are mutually exclusive")
            .with_category(CompileErrorCategory::Usage);
        terminate_cli_error(&error, message_format, None, None);
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
                .unwrap_or_else(|e| terminate_cli_error(&e, message_format, None, None));

            if let Err(e) = result.write_to_path(&output_path) {
                terminate_cli_error(&e, message_format, None, None);
            }
            let metadata_path = default_metadata_path_for_artifact(&output_path);
            if let Err(e) = result.write_metadata_to_path(&metadata_path) {
                terminate_cli_error(&e, message_format, None, None);
            }

            println!("{}: compiled successfully", "success".green());
            println!("  Artifact format: {}", result.artifact_format.display_name());
            println!("  Target profile: {}", result.metadata.target_profile.name);
            println!("  Artifact hash: {:x?}", result.artifact_hash);
            println!("  Output: {}", output_path);
            println!("  Metadata: {}", metadata_path);
        }
        Err(e) => {
            if !should_collect_compile_failure_diagnostics(&e) {
                terminate_cli_error(&e, message_format, Some(&resolved_input), Some(&source));
            }
            let report = compile_path_metadata_with_diagnostics(Utf8Path::new(&input_file), diagnostics_options);
            if report.diagnostics.is_empty() {
                terminate_cli_error(&e, message_format, Some(&resolved_input), Some(&source));
            } else {
                let error = diagnostics_to_cli_error(report.diagnostics);
                terminate_cli_error(&error, message_format, Some(&resolved_input), Some(&source));
            }
        }
    }
}

fn should_collect_compile_failure_diagnostics(error: &CompileError) -> bool {
    error.category == CompileErrorCategory::Compilation && !error.code.as_deref().is_some_and(|code| code.starts_with("CG"))
}

fn requested_message_format() -> MessageFormat {
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        if arg == "--" {
            break;
        }
        if arg == "--message-format=json" {
            return MessageFormat::Json;
        }
        if arg == "--message-format" && args.next().as_deref() == Some("json") {
            return MessageFormat::Json;
        }
    }
    MessageFormat::Human
}

fn requested_color() -> Option<String> {
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        if arg == "--" {
            break;
        }
        if let Some(value) = arg.strip_prefix("--color=") {
            return Some(value.to_string());
        }
        if arg == "--color" {
            return args.next();
        }
    }
    None
}

fn routing_argument(args: &[String]) -> Option<(usize, &str)> {
    let mut index = 0;
    while let Some(arg) = args.get(index) {
        match arg.as_str() {
            "--color" | "--message-format" => index = index.saturating_add(2),
            value if value.starts_with("--color=") || value.starts_with("--message-format=") => index += 1,
            value => return Some((index, value)),
        }
    }
    None
}

fn resolve_primitive_compat(compat: Option<String>, strict: Option<String>) -> Option<String> {
    if strict.is_some() {
        strict
    } else {
        compat
    }
}

fn emit_cli_error(
    error: &CompileError,
    message_format: MessageFormat,
    fallback_file: Option<&Utf8Path>,
    fallback_source: Option<&str>,
) {
    match message_format {
        MessageFormat::Human => print_cli_error_with_source(error, fallback_file, fallback_source),
        MessageFormat::Json => print_cli_error_json(error, fallback_file, fallback_source),
    }
}

fn terminate_cli_error(
    error: &CompileError,
    message_format: MessageFormat,
    fallback_file: Option<&Utf8Path>,
    fallback_source: Option<&str>,
) -> ! {
    emit_cli_error(error, message_format, fallback_file, fallback_source);
    process::exit(error.exit_code())
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

fn print_cli_error_json(error: &CompileError, fallback_file: Option<&Utf8Path>, fallback_source: Option<&str>) {
    let diagnostics = cli_error_diagnostics(error);
    let error_count =
        diagnostics.iter().filter(|diagnostic| diagnostic.severity == cellscript::error::DiagnosticSeverity::Error).count();
    let diagnostic_values =
        diagnostics.iter().map(|diagnostic| diagnostic_json_value(diagnostic, fallback_file, fallback_source)).collect::<Vec<_>>();
    let payload = serde_json::json!({
        "status": "failed",
        "category": error.category.label(),
        "exit_code": error.exit_code(),
        "diagnostic_count": diagnostic_values.len(),
        "error_count": error_count,
        "warning_count": diagnostic_values.len().saturating_sub(error_count),
        "diagnostics": diagnostic_values,
    });
    match serde_json::to_string_pretty(&payload) {
        Ok(json) => eprintln!("{}", json),
        Err(error) => eprintln!("{}: failed to serialize diagnostic JSON: {}", "error".red(), error),
    }
}

fn cli_error_diagnostics(error: &CompileError) -> Vec<&CompileError> {
    if error.related.is_empty() {
        vec![error]
    } else {
        error.related.iter().collect()
    }
}

fn diagnostic_json_value(
    diagnostic: &CompileError,
    fallback_file: Option<&Utf8Path>,
    fallback_source: Option<&str>,
) -> serde_json::Value {
    let runtime_code =
        cellscript::runtime_errors::runtime_error_info_for_diagnostic(diagnostic).map(|info| format!("E{:04}", info.code));
    let file = diagnostic.file.as_ref().map(|file| file.as_str()).or_else(|| fallback_file.map(Utf8Path::as_str));
    serde_json::json!({
        "message": &diagnostic.message,
        "severity": diagnostic.severity.label(),
        "category": diagnostic.category.label(),
        "code": diagnostic.code.as_deref().or(runtime_code.as_deref()),
        "file": file,
        "span": {
            "line": diagnostic.span.line,
            "column": diagnostic.span.column,
            "start": diagnostic.span.start,
            "end": diagnostic.span.end,
        },
        "range": diagnostic_range_json(diagnostic, fallback_source),
        "causes": error_causes(diagnostic),
    })
}

fn error_causes(error: &CompileError) -> Vec<String> {
    let mut causes = Vec::new();
    let mut source = error.source();
    while let Some(cause) = source {
        causes.push(cause.to_string());
        source = cause.source();
    }
    causes
}

fn diagnostic_range_json(diagnostic: &CompileError, fallback_source: Option<&str>) -> serde_json::Value {
    if diagnostic.span.line == 0 || diagnostic.span.column == 0 {
        return serde_json::Value::Null;
    }
    let source = diagnostic
        .file
        .as_ref()
        .and_then(|file| std::fs::read_to_string(file.as_std_path()).ok())
        .or_else(|| fallback_source.map(str::to_string));
    let (end_line, end_column) = source.as_deref().map(|source| line_column_at(source, diagnostic.span.end)).unwrap_or_else(|| {
        let width = diagnostic.span.end.saturating_sub(diagnostic.span.start).max(1);
        (diagnostic.span.line, diagnostic.span.column.saturating_add(width))
    });
    serde_json::json!({
        "start": {
            "line": diagnostic.span.line,
            "column": diagnostic.span.column,
            "offset": diagnostic.span.start,
        },
        "end": {
            "line": end_line,
            "column": end_column,
            "offset": diagnostic.span.end,
        },
    })
}

fn line_column_at(source: &str, byte_offset: usize) -> (usize, usize) {
    let mut line = 1;
    let mut column = 1;
    let capped_offset = byte_offset.min(source.len());
    for (offset, ch) in source.char_indices() {
        if offset >= capped_offset {
            break;
        }
        if ch == '\n' {
            line += 1;
            column = 1;
        } else {
            column += 1;
        }
    }
    (line, column)
}

fn print_single_cli_error(error: &CompileError, fallback_file: Option<&Utf8Path>, fallback_source: Option<&str>) {
    let runtime_info = cellscript::runtime_errors::runtime_error_info_for_diagnostic(error);
    let label = diagnostic_label(error, runtime_info.as_ref());
    if let Some((file, source)) = diagnostic_source(error, fallback_file, fallback_source) {
        eprintln!("{}: {}", colour_diagnostic_label(&label, error), error.message);
        print_source_snippet(file, &source, error);
    } else if error.span.line == 0 {
        eprintln!("{}: {}", colour_diagnostic_label(&label, error), error.message);
    } else {
        eprintln!("{}: {}", colour_diagnostic_label(&label, error), error);
    }

    if let Some(info) = cellscript::runtime_errors::runtime_error_info_for_diagnostic(error) {
        eprintln!("  {}: run `cellc explain E{:04}` for {}", "help".cyan(), info.code, info.name);
    }
    for cause in error_causes(error) {
        if !error.message.ends_with(&cause) {
            eprintln!("  {}: {}", "caused by".cyan(), cause);
        }
    }
    print_followup_hints(error);
}

fn run_top_level_explain(code: String) {
    let command = cellscript::cli::commands::Command::Explain(cellscript::cli::commands::ExplainArgs { code, json: false });
    if let Err(error) = cellscript::cli::commands::CommandExecutor::execute(command) {
        terminate_cli_error(&error, requested_message_format(), None, None);
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
    let line_start = source.split_inclusive('\n').take(line_number.saturating_sub(1)).map(str::len).sum::<usize>();
    let start_in_line = floor_char_boundary(line_text, error.span.start.saturating_sub(line_start).min(line_text.len()));
    let end_in_line =
        floor_char_boundary(line_text, error.span.end.saturating_sub(line_start).min(line_text.len())).max(start_in_line);
    let underline_offset = UnicodeWidthStr::width(&line_text[..start_in_line]);
    let underline_width = UnicodeWidthStr::width(&line_text[start_in_line..end_in_line]).max(1);
    let underline = format!("{}{}", " ".repeat(underline_offset), "^".repeat(underline_width));

    eprintln!(" {} {}:{}:{}", "-->".blue(), file, line_number, column);
    eprintln!("{:>width$} |", "", width = line_width);
    eprintln!("{:>width$} | {}", line_number, line_text, width = line_width);
    eprintln!("{:>width$} | {} {}", "", underline.red(), error.message, width = line_width);
}

fn floor_char_boundary(value: &str, mut index: usize) -> usize {
    while index > 0 && !value.is_char_boundary(index) {
        index -= 1;
    }
    index
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
    if closest_command(arg).is_some() {
        return true;
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
    for command in common_top_level_commands() {
        let about = command.get_about().map(|about| about.to_string()).unwrap_or_default();
        print_command_row(command.get_name(), &about);
    }
    println!("\nDirect options:");
    println!("  -O, --opt <OPT>                  Optimization level 0..3 [default: 0]");
    println!("  -o, --output <FILE>              Write artifact to FILE");
    println!("  -d, --debug                      Include debug metadata where supported");
    println!("  -t, --target <TARGET>            Target: riscv64-asm or riscv64-elf");
    println!("      --target-profile <PROFILE>   Target profile: ckb");
    println!("      --message-format <FORMAT>    Diagnostic format: human or json [default: human]");
    println!("      --color <WHEN>               Colour output: auto, always, or never [default: auto]");
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

fn common_top_level_commands() -> Vec<clap::Command> {
    let mut commands = cellc_cli_command()
        .get_subcommands()
        .filter(|command| !command.is_hide_set() && command.get_display_order() < 200)
        .cloned()
        .collect::<Vec<_>>();
    commands.sort_by(|left, right| {
        left.get_display_order().cmp(&right.get_display_order()).then_with(|| left.get_name().cmp(right.get_name()))
    });
    commands
}

fn print_command_list() {
    println!("Installed cellc commands:\n");
    for command in cellc_cli_command().get_subcommands().filter(|command| !command.is_hide_set()) {
        let about = command.get_about().map(|about| about.to_string()).unwrap_or_default();
        print_command_row(command.get_name(), &about);
    }
}

fn print_command_row(name: &str, about: &str) {
    println!("  {:<24} {}", name, about);
}

fn cellc_cli_command() -> clap::Command {
    cellscript::cli::commands::CliParser::command()
}
