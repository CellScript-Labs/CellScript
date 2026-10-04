# Issue #22 acceptance map

This map covers the first exact profile on 0.32. The maintainer waived human
independent review on October 3 and authorized completion on October 4.
Release publication, public-network deployment and Phase 3 ecosystem expansion
remain separate. The [profile](../../CELLSCRIPT_ZK_PROFILE.md) is the normative
implementation contract. Final gate provenance is recorded in `zk-validation.json`.

| #22 criterion | Executable evidence / scope |
| --- | --- |
| Accepted first application/profile | Profile document; private-counter README; setup matrix digest, VK and manifests in `private-counter/` |
| Named exact dependency | `src/zk_package.rs`; `tests/zk_transition.rs` exercises real Cell.toml, metadata diagnostics, source-index/receipt/VK substitution, duplicate and missing declarations; parent exporter uses this path |
| Opaque proof and canonical public inputs | Nominal source types; shared allocation-free `zk.rs`; `tests/zk_vectors.rs` and generated TypeScript codec vectors |
| Source expression/span retention | `zk_metadata_preserves_exact_contract_and_source_origins`, WASM metadata test; `zk_profile::statement_origins`; checker reconstructs from typed semantics |
| Result enforcement | IR permits one unconditional Unit call; checker binds preflight, Spawn/IPC/Wait and rejection; conditional/duplicate/ignored-result source negatives and child transport tests |
| Incomplete production contracts reject | Nominal/literal type checks and IR validation before codegen; named package checks before codegen; independent checker checks emitted coverage and fixed budgets |
| Reproducible artifacts | `contracts/zk-private-counter/reproduce.sh`: two independent Cargo target directories for child/lifecycle and two fresh compiler processes for the same parent fixture; exact-source pinned toolchain, not a cross-host claim |
| Checker mutations | `exact_zk_machine_mutations_reject_after_hash_rebinding`: over 150 machine mutations plus VK, request width, call/cycle bounds and source/context metadata mutations; standalone checker also checks scratch locations and control flow |
| Real child positive/negative VM cases | `contracts/zk-transition-verifier/host` tests plus private-counter scheduler tests; modeled transport and real scheduler evidence are separate |
| Stateful transaction binding | `private-counter/node.json`: unique initialization, two committed updates, malformed successor and cryptographic replay rejection with newly derived transaction context |
| Cross-SDK vectors | `tests/fixtures/zk-wire-vectors.json`; Rust and TypeScript compare all bytes of statement, scalar inputs and request; TS rejects every altered PI byte; native adapter derives from finalized raw transaction |
| Resources/environment | `private-counter/scheduler.json`, `resources.json`, `reproducibility.json`, node and setup records: exact ELF/key/circuit identities, cycles, occupied state capacity, transaction/witness bytes, per-VM/EXEC observed stack and explicit child allocator budget; stack observations are not heap peaks |
| Product agreement | Standard call syntax/formatter, nominal type checker/IR/metadata, LSP tests, VS Code types/snippet, generated TypeScript and Rust adapter; Registry reuses existing runtime_verifier/ckb_executable/tcb role; website renders the updated Tutorial 06 from docs/wiki; gates execute vectors, reproduction and resources |
| Independent security review | **Waived, not performed.** This item is not represented as a passed review. Software artifact checking and all execution gates remain required. |
| Evidence levels and terminology | Profile, application/verifier READMEs and this index distinguish ProofPlan, structural checking, modeled VM, actual scheduler, local-node admission and public deployment |

The current application admission is engineering acceptance under a trusted
operator/host single-party setup. It does not extend to an arbitrary circuit,
other proof systems, multi-Cell transitions, new VKs or future verifier upgrades.
The PK archive is distributed separately; no secret witness or setup seed is
committed. Existing fixture code Cells use disposable local-chain custody.

## Reproduction

Run the repository's `dev`, `ci` and `backend` gates with the pinned CKB checkout.
Set `CELLSCRIPT_COUNTER_PACKAGE` to the matching complete setup package to rerun
the candidate VK; without it the scheduler uses a clearly labeled public test
setup. The node test requires the clean-source pinned-CKB acceptance run.
The separate resource replay consumes the application test's exported fixture
and requires the ordinary group verifier's verdict and cycle total to match.
