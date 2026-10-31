# Native certification integration

Status: command boundary extracted; final alpha.2 integration acceptance is pending.
Date: 2026-10-30. Scope: alpha.2 A2-11.

## Ownership

| Owner | Responsibility |
| --- | --- |
| [Command coordination](../src/cli/commands.rs) | Register `certify`, select typed arguments and dispatch |
| [Certification adapter](../src/cli/certification.rs) | Own the argument declaration, plugin selection, report I/O, summary and exit decision |
| [NovaSeal engine](../src/cli/novaseal_certification.rs) | Reconstruct proposal-specific profile/evidence checks from the requested repository |
| [CLI output](../src/cli/output.rs) | Shared JSON input errors, human/machine presentation and global output policy |

Compiler semantics and the independent artifact checker do not call this native
integration. The CLI tree is excluded from the browser `wasm` profile. The adapter
depends on the engine and shared output, without importing command-coordination
helpers back. No alternative implementation or compatibility re-export remains
in `commands.rs`.

`CertifyArgs` uses the already pinned Clap derive dependency. One declaration
supplies registration and extraction; paths still require UTF-8 CLI arguments.
Global JSON handling remains shared. Argument extraction returns a usage
diagnostic instead of adding an unchecked expectation to this boundary.

## Command, packaging and release inventory

`cellc certify --plugin novaseal-profile-v0` remains advertised, with a required
plugin and unchanged options, defaults, help, human/JSON results and exit codes.
Unknown plugins reject before evidence generation. The engine ID and source path
remain `cellscript::cli::novaseal_certification` and
`src/cli/novaseal_certification.rs`; report and actual engine-source hashes retain
their existing binding. This extraction creates no certification or attestation.

The engine reads proposal manifests, schemas, verifier/provenance sources,
fixtures and local/external evidence under the supplied repository root.
Checking an existing report remains distinct from generating one. The root
Cargo package includes native `src/` files but excludes `proposals/`; an installed
compiler does not contain a proposal checkout or its evidence. Release checks
still reference the engine source, NovaSeal pins, verifier builds and acceptance
boundaries. Package exclusions, dependency versions and release prerequisites
are unchanged. No publication or production-equivalence claim follows.

## Dependencies and validation

`sha2` also serves package/Registry integrity, workspace/upgrade identities,
package inspection and native artifact tooling. Tests have an assembler-cost
consumer. Moving this one consumer does not justify dependency removal or
optionality. Native build time, peak memory and shipped bytes are measured
separately from browser raw/gzip size in the alpha.2 backlog. Warmed dependency
builds are identified; moved source lines are not binary-size savings.

All 19 command tests pass, including a typed diagnostic for a lockfile missing
after generation. Seven real before/after CLI cases cover help, missing
arguments, unsupported plugins, global JSON, missing reports and an invalid
supplied report. Exit codes, stdout and stderr are byte-identical; receipts are
under `target/alpha2-work/effect-facts-migration/certification-boundary/`.
All-target checks and clippy pass. The preceding runtime extraction passes all
1,129 compiler-library tests; the command tests cover the subsequent adapter
change separately. Final-source backend/dev/CI acceptance remains under A2-14.
These checks do not publish, deploy, run TAC or run Daybreak.
