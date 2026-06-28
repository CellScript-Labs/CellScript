# CellScript 0.21 CLI UX Reorganisation Plan

**Status**: Planning
**Scope**: command discovery, canonical command grouping, compatibility aliases,
diagnostic transport, MCP/skill alignment, and parser-maintenance risk
reduction
**Depends on**: 0.20 CLI discovery improvements, package/build/deployment
identity tooling, and the existing `cellc --help` / `cellc --list` split

## Goal

CellScript 0.21 should keep the current audit and deployment tooling surface,
but make it easier to discover and harder to maintain incorrectly.

The CLI should have one canonical command tree, visible help that matches that
tree, and a compatibility path for existing scripts. The release should reduce
top-level command noise without removing compiler, registry, transaction,
deployment, or certification capabilities.

The same command tree and diagnostic contract should be reusable by the 0.21
CellScript MCP server and programming skills. Agent tooling should ask the
compiler and documentation for facts instead of maintaining a separate command
map or stale prompt-only workflow.

The target user experience is:

```text
cellc --help
  -> curated entry point for common package and direct-source workflows

cellc --list
  -> canonical stable command tree

cellc <group> <verb>
  -> grouped audit, transaction, deployment, registry, package, and explain
     workflows

legacy flat commands
  -> hidden compatibility aliases with migration diagnostics during the 0.21
     cycle
```

## Baseline

The current `cellc --list` surface exposes 51 top-level commands in this
checkout. The complexity is not caused by useless commands; most commands map
to real compiler, metadata, builder, registry, deployment, or CKB acceptance
workflows. The problem is that too many related workflows are exposed as flat
top-level commands, and some workflows have both flat and nested spellings.

Important current issues:

- `registry verify` and `registry-verify` are equivalent;
- `package verify` and `package-verify` are equivalent;
- `registry add` and `registry-add` are equivalent;
- `login`, `auth login`, and `auth capability create` overlap in intent;
- `explain`, `explain-profile`, `explain-proof`,
  `explain-assumptions`, and `explain-generics` occupy separate top-level
  slots;
- `validate-tx`, `solve-tx`, and `trace-tx` are transaction workflows but are
  not grouped under a transaction namespace;
- `deploy-plan`, `verify-deploy`, and `diff-deploy` are deployment workflows
  but use mixed verb/noun ordering;
- the top-level help path is hand-curated while full command discovery comes
  from the clap command tree;
- several low-level or experimental flags have incomplete help text;
- the hand-written clap builder plus hand-written `parse_matches` mapping makes
  argument drift easy when new fields are added.

## Non-Goals

- Do not reduce CellScript's audit, registry, deployment, or certification
  capability.
- Do not remove legacy flat commands without a release-bound compatibility
  window.
- Do not replace the useful curated top-level `cellc --help` entry point with a
  raw exhaustive clap dump.
- Do not silently change non-TTY text output into JSON output.
- Do not rename the existing successful-output `--json` flag.
- Do not split or refactor NovaSeal implementation code as part of CLI UX work.
- Do not make CLI reorganisation a backend, CKB semantics, or artifact-format
  change.
- Do not let the MCP server or programming skills define commands, diagnostics,
  or evidence levels that are not present in the canonical CLI/docs surface.

## Canonical Command Tree

0.21 should make nested command groups canonical and treat flat spellings as
compatibility aliases.

### Explain

```text
cellc explain <code>
cellc explain profile <profile>
cellc explain proof [INPUT]
cellc explain assumptions [INPUT]
cellc explain generics [INPUT]
cellc explain graph [INPUT]
```

Legacy aliases:

```text
cellc explain-profile      -> cellc explain profile
cellc explain-proof        -> cellc explain proof
cellc explain-assumptions  -> cellc explain assumptions
cellc explain-generics     -> cellc explain generics
cellc explain-graph        -> cellc explain graph
```

### Transaction

```text
cellc tx validate --against <METADATA> --tx <TX>
cellc tx solve [INPUT]
cellc tx trace --against <METADATA> --tx <TX>
```

Legacy aliases:

```text
cellc validate-tx  -> cellc tx validate
cellc solve-tx     -> cellc tx solve
cellc trace-tx     -> cellc tx trace
```

### Deployment

```text
cellc deploy plan [INPUT]
cellc deploy verify --plan <PLAN>
cellc deploy diff --old <PLAN> --new <PLAN>
cellc deploy lock-deps [INPUT]
```

Legacy aliases:

```text
cellc deploy-plan    -> cellc deploy plan
cellc verify-deploy  -> cellc deploy verify
cellc diff-deploy    -> cellc deploy diff
cellc lock-deps      -> cellc deploy lock-deps
```

### Registry And Package

```text
cellc registry verify
cellc registry add
cellc registry edit
cellc package verify
```

Legacy aliases:

```text
cellc registry-verify  -> cellc registry verify
cellc registry-add     -> cellc registry add
cellc package-verify   -> cellc package verify
```

### Authentication

```text
cellc auth capability create
cellc auth capability submit
cellc auth capability revoke
```

`cellc auth login` may remain as a visible user-friendly alias only if it keeps
a distinct beginner-facing purpose. Otherwise it should be hidden and routed to
`cellc auth capability create`.

The flat `cellc login` command should be hidden in 0.21 and retained only as a
compatibility alias during the deprecation window.

## Phased Work

### P0: Help And Compatibility Hygiene

Required work:

- fill all visible flag and positional help strings;
- keep the curated top-level `cellc --help`, but make the common-command list
  come from one canonical registry rather than a detached string array;
- mark redundant flat commands as hidden compatibility aliases;
- emit a short migration warning when a hidden legacy alias is used in an
  interactive terminal;
- add help regression tests that fail when a visible flag has an empty help
  string;
- keep `cellc --list` focused on canonical commands.

Acceptance:

- `cellc --help` remains short and useful;
- `cellc --list` no longer promotes duplicate flat aliases;
- existing scripts using flat aliases still execute during the compatibility
  window;
- all visible auth, registry, publish, install, transaction, and deployment
  flags have non-empty help text;
- documentation examples prefer canonical nested commands.

### P1: Grouped Command Surface

Required work:

- add the canonical `explain`, `tx`, and `deploy` subcommand groups;
- route old flat commands to the same internal handlers through compatibility
  aliases;
- update tutorials, roadmap examples, README command tables, release notes, and
  gate-policy examples that mention the affected commands;
- add parser tests for canonical and legacy spellings;
- keep JSON output schemas unchanged unless a command explicitly documents a
  schema revision.

Acceptance:

- a new user can discover related workflows by command group;
- legacy and canonical spellings produce equivalent command results, apart from
  the migration warning;
- command count visible through `cellc --list` drops materially without losing
  capability;
- command names use noun group plus verb consistently, for example
  `deploy verify` rather than `verify-deploy`.

### P1: Parser Maintenance Reduction

Required work:

- migrate one command group at a time from hand-written clap builder plus
  hand-written `parse_matches` mapping to `#[derive(Args)]` /
  `#[derive(Subcommand)]`;
- start with low-risk grouped surfaces such as `registry`, `package`, and
  `auth`;
- then migrate `explain`, `tx`, and `deploy`;
- keep `Command` execution handlers stable while the parser layer changes;
- avoid broad formatting churn in the large `src/cli/commands.rs` file.

Acceptance:

- adding a new argument requires one typed field and one help string, not
  duplicated builder and parser edits;
- existing command tests keep passing;
- command-specific JSON summaries keep their existing successful-output
  schemas;
- migration does not change backend, metadata, registry, deployment, or CKB
  semantics.

### P2: Diagnostic Transport Contract

Required work:

- add `--message-format=json` for diagnostics and errors;
- keep `--json` for successful command result payloads;
- include at least `code`, `message`, `rendered`, `spans`, `help`, and
  `applicability` where that information exists;
- add `--color=auto|always|never` and respect `NO_COLOR`;
- keep non-TTY text output stable unless the user explicitly asks for
  `--message-format=json`;
- document stdout/stderr boundaries so machine-readable success data and
  diagnostics do not collide.

Acceptance:

- CI tools can consume diagnostics without scraping coloured text;
- human-readable rendering remains available and is embedded in the `rendered`
  diagnostic field;
- existing `--json` consumers are not forced to rename flags;
- colour behaviour is deterministic across TTY, non-TTY, and `NO_COLOR`
  environments.

### P2: Shell Completion And Long Help

Required work:

- add `cellc completions <shell>` that writes shell completion output to stdout;
- do not edit shell rc files automatically;
- consider generated man pages only after the canonical command tree is stable;
- keep completion generation out of the release evidence path unless it becomes
  part of the packaged artifact contract.

Acceptance:

- completions can be generated for supported shells without side effects;
- generated completions expose canonical commands and omit hidden legacy aliases
  unless an explicit all/legacy mode is requested.

### P2: MCP And Programming Skill Alignment

Required work:

- expose canonical command discovery in a machine-readable form that a
  CellScript MCP server can consume without scraping rendered help;
- make `--message-format=json` diagnostics suitable for both editor clients and
  MCP tool responses;
- provide stable descriptions for evidence boundaries such as compile-only,
  builder-backed, node dry-run, tx-pool accepted, submitted, and externally
  attested;
- ensure CellScript programming skills teach canonical 0.21 command groups and
  mention legacy flat aliases only as migration paths;
- keep skill examples aligned with current docs, examples, and gate policy.

Acceptance:

- the MCP server can discover commands, arguments, output modes, and legacy
  alias hints from the same canonical source used by CLI help;
- skills do not contain private copies of stale command tables;
- skill workflows preserve stdout/stderr and `--json` /
  `--message-format=json` boundaries;
- agent-facing tools cannot claim production readiness without the evidence
  required by the 0.21 roadmap and gate policy.

## Deprecation Policy

0.21 should use a conservative compatibility policy:

1. introduce canonical nested commands;
2. hide duplicate flat commands from ordinary discovery;
3. keep flat commands executable for at least one release cycle;
4. print migration warnings only when they will not corrupt machine-readable
   stdout;
5. remove flat commands only after release notes and migration docs have named
   the removal version.

Warnings must go to stderr. JSON success output must stay on stdout.

## Documentation Updates

When this plan is implemented, update:

- `README.md` command examples;
- `docs/wiki/Tutorial-04-Packages-and-CLI-Workflow.md`;
- `docs/wiki/Tutorial-06-Metadata-Verification-and-Production-Gates.md`;
- `docs/wiki/Tutorial-07-LSP-and-Tooling.md` if completions or diagnostics are
  documented there;
- `docs/CELLSCRIPT_GATE_POLICY.md` when gate examples change;
- release notes for the first release that hides or removes legacy aliases.

## Validation

Routine documentation and CLI parser changes should pass:

```bash
./scripts/cellscript_gate.sh dev
```

Parser migrations or diagnostic transport changes should also run focused CLI
tests before the full gate:

```bash
cargo test --locked -p cellscript --test cli -- --test-threads=1
```

Merge-readiness remains:

```bash
./scripts/cellscript_gate.sh ci
```

No CLI UX change should be described as release-ready unless the canonical
command tree, legacy aliases, help output, docs, tests, and gate behaviour agree.
