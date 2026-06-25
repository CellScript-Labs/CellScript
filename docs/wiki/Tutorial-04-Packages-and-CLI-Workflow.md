Small experiments can be compiled as single `.cell` files. Once a contract has
more than one source file, a dependency, or a release target, use a package.

A package gives the compiler a stable place to find the entry file, build
settings, dependencies, and lockfile. That makes builds repeatable for you, and
reviewable for someone else.

## What You Will Learn

- how to create a package;
- what belongs in `Cell.toml`;
- how to build, check, format, and document a package;
- which reports are useful during review;
- where the current package workflow intentionally stops.

## Create a Package

Create an application-style package:

```bash
cellc init my_contract
cd my_contract
```

This creates a `Cell.toml` manifest and a source entry. Use this form when you
want a contract package with a concrete entry.

Create a library-style package:

```bash
cellc init my_lib --lib
```

Ask for a machine-readable summary when scripting:

```bash
cellc init my_contract --json
```

## Read The Manifest

A minimal manifest looks like this:

```toml
[package]
name = "my_contract"
version = "0.1.0"
edition = "2021"
entry = "src/main.cell"
source_roots = ["src"]

[build]
target = "riscv64-elf"
target_profile = "ckb"
out_dir = "build"

[dependencies]
my_lib = { path = "../my_lib" }
```

Read the manifest as a build promise:

- `entry` tells the compiler where the package starts;
- `source_roots` tells the compiler which package directories contain `.cell`
  modules;
- `target` chooses assembly or ELF-style output;
- `target_profile` chooses the runtime assumptions;
- `out_dir` chooses where artifacts are written;
- path, git, and registry source-package dependencies keep package inputs
  explicit and lockable.

Registry source-package resolution is implemented for packages that provide
`Cell.toml`, `registry.json`, tag-pinned Git provenance, and a verified
`source_hash`. Local path dependencies remain the fastest repeatable
development workflow, and non-CellScript registry artifact profiles still fail
closed until they have their own resolver contracts.

## Multi-file Packages

Package builds are entry-driven, but the frontend loads the full package source
set before compiling the entry artifact. The compiler walks `source_roots`
(defaulting to `src`), parses every `.cell` file it finds, registers each file's
`module` declaration, and validates every `use path::Symbol` import against the
loaded module graph. Path dependencies are loaded the same way, so shared schema
packages can provide common Cell types without copying them into every contract.

There is no `mod` keyword and no implicit basename lookup. The module declared
inside the file is the source identity. Duplicate module declarations fail, bad
imports fail, and invalid package modules fail during `build` or `check` even
when the entry file does not reference them directly.

This is not a contract linker. Each CKB script remains an independent RISC-V
artifact. Cross-file helper calls are resolved at compile time and inlined into
the entry artifact, but there is no ELF linker and no cross-script runtime
coupling. Use multi-file packages for schema reuse, shared helper functions,
reviewable module organization, and repeatable source/package hashes.

For registry resolution, `cellc add` must remain a dependency
resolver, not a code-snippet finder. Anything reachable by `cellc add` must be
safe to participate in the package, build, deployment, or declared TCB identity
chain. Template-only material belongs behind copy/scaffold commands instead.

## Build

Run the package build:

```bash
cellc build
```

Useful flags:

```bash
cellc build --target riscv64-asm
cellc build --target riscv64-elf
cellc build --target-profile ckb
cellc build --production
cellc build --json
```

`build` reads `Cell.toml`, compiles the current package entry, and writes the
artifact plus metadata sidecar under the configured output directory.

For a one-off source file, use the top-level compiler form instead:

```bash
cellc path/to/file.cell
```

That form is great for quick experiments. Packages are better when you need
repeatability.

## Check Without Writing Artifacts

Use `check` when you want fast feedback:

```bash
cellc check
cellc check --all-targets
cellc check --target-profile ckb
cellc check --production
cellc check --deny-runtime-obligations
cellc check --json
```

`check --all-targets` is useful before committing. It catches source and profile
problems without producing build artifacts.

## Format And Generate Docs

Format the package:

```bash
cellc fmt
cellc fmt --check
cellc fmt --json
```

Generate package docs:

```bash
cellc doc
cellc doc --json
```

Generated docs summarize modules, actions, resources, receipts, locks,
flow rules, and lowering metadata.

## Audit And Evidence Reports

When a package is ready for review, ask the compiler for the facts it already
knows:

```bash
cellc metadata . --target riscv64-elf --target-profile ckb -o build/main.metadata.json
cellc constraints . --target riscv64-elf --target-profile ckb -o build/main.constraints.json
cellc abi . --target-profile ckb
cellc scheduler-plan . --target-profile ckb --json
cellc opt-report . --target riscv64-elf --target-profile ckb --json
```

For CKB-specific builder and deployment review:

```bash
cellc constraints . --target riscv64-elf --target-profile ckb --json
cellc abi . --target-profile ckb --action transfer
cellc entry-witness . --target-profile ckb --action transfer
cellc ckb-hash --file build/main.elf
cellc verify-artifact build/main.elf --expect-target-profile ckb --verify-sources --production
```

Builder-facing contract commands such as `action build`, `entry-witness`,
`solve-tx`, `explain-assumptions`, `validate-tx`, and `gen-builder` expose the
metadata that transaction builders consume. Prefer `--json` where a command
offers it, and reserve human summaries for interactive review.

These reports are not busywork. They answer questions reviewers will ask:

- what is the entry ABI;
- what witness layout is expected;
- what capacity or runtime obligations remain;
- what CKB hash policy is being used;
- whether the artifact still matches the source and metadata.

They do not replace chain acceptance reports, builder-generated transactions,
occupied-capacity evidence, or CKB production gates.

## Local Dependencies

Add a local dependency:

```bash
cellc add my_lib --path ../my_lib
```

`add --path` records the dependency in `Cell.toml`. To resolve the dependency
graph and write `Cell.lock`, run:

```bash
cellc install
```

You can also add and lock a local dependency in one command:

```bash
cellc install my_lib --path ../my_lib
```

The current CLI can record a Git dependency URL:

```bash
cellc add math --git https://example.com/math.git
cellc install math --git https://example.com/math.git
```

For reviewable package identity, prefer a manifest-level detailed dependency
with `rev = "<full-commit-hash>"`, then run `cellc install` so `Cell.lock`
records the resolved package source. Branch, tag, and default-branch Git
dependencies are easier to move without changing `Cell.toml`, so treat them as
development convenience rather than production evidence.

Remove it:

```bash
cellc remove my_lib
```

`install`, `update`, and normal dependency removal refresh the lockfile so
direct and transitive local path dependencies stay consistent.

## Registry Resolver Boundaries

CellScript's registry design follows the same split as the package identity
model:

- package identity answers which source was referenced;
- build identity answers which artifact and metadata were produced;
- deployment identity answers which CKB Cell, CellDep, or runtime artifact is
  being used.

Registry discovery can be broad. It may index CellScript source packages,
runtime verifiers, deployed CKB artifacts, reproducible artifacts, and even
external CKB tooling artifacts such as bootstrapper outputs. Resolver profiles
must stay narrower: an object can be discovered without being installable by
`cellc add`.

That means registry resolution is stricter than discovery. Current `cellc`
registry dependencies are CellScript source-package dependencies. Future
profile-aware resolver paths should accept only objects that can be checked
fail-closed:

| Kind | Current `cellc add` | Future profile boundary |
| --- | --- | --- |
| `source_package` / library | yes | Source and API identity must be pinned and reproducible. |
| `runtime_verifier` / `spawn-verifier` | no, unless wrapped as a CellScript package today | TCB object; requires verifier ID, ABI, artifact identity, build profile, security status, and production deployment pins when used in production. |
| `deployable_contract` | no, unless it is a CellScript source package today | Must expose build/audit/deployment identity, not just source text. |
| `deployed_artifact_record` | no | Must bind network, OutPoint, dep type, code/data hash, and status. |
| `reproducible_artifact` | no | Must bind source hash, build profile hash, artifact hash, and compatibility profile. |
| `protocol_profile_library` | only if it is a real CellScript package today | Must be a real package with checkable source/schema/API semantics. |
| `template`, `cookbook`, `protocol_skeleton`, scaffold | no | Copy-only starting material; after copying, it becomes local project code. |

The rule is intentionally blunt:

```text
Discovery can be broad; dependency resolution is narrow.

Anything reachable by cellc add must be dependency-safe, artifact-safe,
deployment-fact-safe, or declared-TCB-safe.

Anything scaffold-only must be copied, not resolved.
```

For example, a BIP340 verifier package can have no business parameters and
still be resolver-safe because it is a runtime verifier artifact. Its manifest
or registry record must identify the verifier capability, IPC ABI, artifact
hashes, build profile, TCB/security status, and any production CellDep pins.

A NovaSeal starter project, by contrast, is not dependency-safe merely because
it contains useful `.cell` code. If users are expected to copy it and edit terms,
authorities, manifests, or deployment pins, it belongs in a cookbook or template
flow, not in dependency resolution. The current `cellc` CLI does not ship a
template or cookbook-copy command; copy starter material with repository tooling
or a future scaffold command, then treat the result as local project source.

It should not be installed with:

```text
cellc add novaseal/mvb-starter
```

This keeps the registry as a verifiable dependency and artifact discovery layer,
not a general examples marketplace.

For mixed projects, keep the records separate. A CellScript app may depend on a
CellScript library, reference a deployed verifier as TCB evidence, use a
reproducible bootstrapper artifact during its build process, and copy a cookbook
starter into local source. Those are four different profile boundaries. They
may share one registry service and one `namespace/name` style, but they must not
share one unchecked dependency path.

## Package Information

```bash
cellc info
cellc info --json
```

Use `info` when you want a quick view of the package boundary before building or
debugging dependency resolution.

## Experimental Commands

Registry source-package installation and registry-backed `update` are supported
for the CellScript source-package profile. The public registry policy is:
`cellc auth capability create --principal-id <principal_id> --scope
publish:namespace/package --expires 90d` authorises a JoyID-rooted publisher
capability, then `cellc publish` writes a real registry entry. The
`principal_id` is derived from the connected JoyID signer, not from a display
address. The same metadata can still be
mirrored with `cellc publish --offline` to `registry.json` and Git tags for
audit, local fixtures, and offline fallback. `cellc registry add` manages discovery/claim metadata rather than
ordinary version publication. `run`, `repl`, cryptographic audit-signature
verification, and non-CellScript artifact profiles remain future-facing or
fail-closed.

## Next

With a repeatable package workflow in place, continue with
[CKB Target Profiles](https://github.com/CellScript-Labs/CellScript/wiki/Tutorial-05-CKB-Target-Profiles).
