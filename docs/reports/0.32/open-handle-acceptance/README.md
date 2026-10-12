# Compatible-open participant acceptance record

Status (2026-10-12): implementation, full CI and clean-source backend acceptance
passed. Technical acceptance for issues #28 and #29 is complete within the
bounded profiles below.

Implementation: `bb2981d5fe4cebe9931173014139e0d0a6422340`.
Test base: `5323f391d0b81f251662fa8028240df25b820fb5`, plus the anchor/inventory
refresh and preserved `code policy` diagnostic prefix. This three-file binary
Git diff has SHA-256
`d78b8a78bee61c841378a86accdb7a6581913604c9fc21958748ee9a3f193099`.
The resulting tracked source tree is `80069d275bef6d7dfab5cf99bb4153f3daa6b237`
(before this acceptance document). Signed source commit
`16b9e58394aa182f071303e6b5ab15dc94d304f4` contains those exact changes and this
acceptance document's initial record. The refresh changes the anchor's ProtocolBundle hash because generated
builder manifests now retain Lock declarations; its artifact, raw/serialized
transaction, cycle, stack, witness and capacity assertions remain unchanged.
External human review for #28 and #29 was waived on 2026-10-11; no external
audit, release publication, deployment or live-network inclusion is claimed.

The normative [design record v2](../../../CELLSCRIPT_OPEN_HANDLE_DESIGN.md)
specifies the admitted public codecs, immutable policy semantics and threat
model. Nominal `ScriptHandle<I>` and `VerifierHandle<I>` select independently
checked implementations. Lock and Type consumers use the same committed-args
root and canonical witness transport. Participants cover Type-policy, single
Type/Lock and parameterless SPAWN/WAIT child profiles. Unsupported codecs
reject instead of receiving partial receipts.

## Acceptance mapping

| Required boundary | Executable evidence |
| --- | --- |
| Complete public receipts, nested layouts, source ownership, callable/witness ABI, effects and target | `tests/policy_artifact_checker.rs`: actual four-file receipts and independently rebound mutation tests; `tests/frozen_interface.rs`: frozen catalogs and defining-owner source proofs |
| Exact/compatible selection, active/yanked members, floors, stale roots and all-member validation | `crates/cellscript-artifact-checker/tests/open_handle_policy.rs` and source-policy tests in `tests/frozen_interface.rs` |
| Selected code bytes, exact CellDep, complete Script, network and Type-ID lineage | Direct-dependency/Type-group checker tests, `source_policy_binds_type_id_history_and_the_exact_final_dependency`, and VM identity/substitution tests in `tests/exact_script_handles.rs` |
| Lock/Type consumers, selected Lock/verifier execution and byte parity | `open_lock_and_verifier_profiles_materialize_and_execute_identical_transactions`: 16 O0–O3 combinations, identical native/CLI reports, generated TypeScript witnesses and SDK transactions; all actual Script groups execute |
| Selected participant rejects despite valid interface admission | False Lock predicate rejects its input group; failing spawned child rejects the parent Lock/Type group |
| Independent decoder and child-exit evidence | Single-entry wrapper/normalizer/parameter mutations in `tests/policy_artifact_checker.rs`; rebound helper failure-to-success machine mutation in the Lock/verifier integration test |
| No unbound or conditional verifier claim | `open_verifier_target_claim_rejects_unbound_or_conditional_delegation`: missing membership, another dependency, invocation before checking, conditional checking and multiple raw invocations reject at O0–O3 |
| Foreign observations, source/schema/cardinality and missing peer groups | Existing open-participant VM and signed Type-policy integration matrices plus missing actual Lock group and rebound role/dependency/group/claim tests |
| Signing and post-build substitution | `open_protocol_admission_binds_private_receipts_to_complete_transaction_bytes`: real SDK secp256k1 signing, unsigned rejection, signed peer rejection, witness substitution and adapter signing-preparation checks |
| Independent CCC wire parity and immutable identities | Authorization-set CCC vectors, deterministic receipt reconstruction and complete raw/serialized transaction identity comparisons |
| Metadata, editor and generated-builder scope | Typed nominal witness parameters in actions/Locks; LSP type/helper completions; generated manifest Lock declarations and nominal-class witness transport |

## Frozen bounds

Selections contain exactly 656 bytes, with a 664-byte CSARG payload. Policies
contain 1–32 members; native transactions admit at most 32 bindings, 64 direct
dependencies and 256 inputs/outputs/witnesses. Dep groups and ambiguous selected
code identities reject. Input preflight caps each item at 4 MiB and shared input
at 16 MiB. Public layout nesting is bounded to eight and width to 512 bytes;
directly decoded Cell fields stay flat unsigned scalars.

The single-entry wrapper has a 5,376-byte frame and a 4,096-byte witness ceiling.
The independently checked hex4 SPAWN/WAIT helper has a 624-byte frame and at most
256 input bytes. The open-participant VM corpus enforces a 3,000,000-cycle ceiling;
the 64-dependency corpus exercises the traversal boundary. This is a regression
budget for the declared fixtures, not a cycle guarantee for arbitrary application
predicates. Child invocation records never claim execution before a dry-run.

## Unified gates

The ordinary commit gate and focused open-participant tests passed before the
implementation commit. Full CI passed against the exact source tree above,
including 42 frozen-interface tests, 77 policy artifact-checker tests, the
trusted-external regression suite, clippy, independent CCC vectors, Cargo package
verification, Registry services and the website build.

The full backend gate passed on signed source commit `16b9e583` in a clean
checkout: compiler/checker tests, clippy, dual-backend package scenarios, release
proof/application/resource regressions, machine contracts, all 183 syntax
combinations, pinned CKB stateful scenarios and `counter_node_acceptance`.
The CKB receipt binds that exact commit with `git_dirty = false`, a clean pinned
CKB checkout, and fresh runtime binary SHA-256
`3aa055152d45c3a9aab36c8227d1f20b12ea17acc1df89f40a3ba294c7d36286`.
The hardening gate reports no failures; its resource identity evidence remains
`always-success-fixture-only`, not an arbitrary application resource guarantee.
The node test verified the receipt, source identity and binary identity before
executing its local-node transaction acceptance. The earlier dirty-source refusal
is superseded by this successful clean replay. This final record changes only
documentation; its parent source commit is the executable acceptance identity.

Logs are retained locally under `target/open-handle-acceptance-80069d27/`.
Their SHA-256 identities are:

| Evidence | SHA-256 |
| --- | --- |
| Full CI, exit 0 | `17e9f16143a23afd481f1e1bb05abe54ebc364c891d52998d0f7260e0db2d846` |
| Full backend including node acceptance, exit 0 | `d768615c34b8054b907448347606b9e01fdfbbd7206101a8352ed2296b191269` |
| Strict backend audit, passed | `093138c609ab64847338683c6c34117a33fb6738e01bf6c480322eaee6fa12cf` |
| Clean-source CKB acceptance receipt, passed | `a144be567c14c35702034370f7ccc1c76ded743441560528a45425829a36908c` |
| Focused checker and trusted-external suites, 77 + 8 passed | `eeda94bf1fae65951a5796d10b263323e5ae0fed4ed2aa3448cad28e6ada3671` |
| Rebound header/member substitution matrix, passed | `edf434a60f5773adfcb2607f583cc3a42abae56cf189e14696a8ee6e34bd42d7` |

The commands are `./scripts/cellscript_gate.sh ci` and
`./scripts/cellscript_gate.sh backend`. Backend uses the pinned CKB checkout at
`f7fa4436737756f97a24e254f22c13a36316ecea`. Separate build directories prevent
the parallel gates from replacing each other's compiler and checker binaries.
