# CellScript Transactional Upgrade Plans

**Status**: active Edition 2027 contract for issues #20 and #98.

CellScript separates dependency selection from lockfile mutation. `cellc
update-plan` resolves and compiles a candidate graph in memory, then emits a
reviewable `cellscript-upgrade-plan-v2` document. `cellc update` has the same
planning behavior by default. Only `cellc update --apply-plan <file>` can
replace `Cell.lock` files.

## Plan An Upgrade

```bash
cellc update-plan . --offline --output target/upgrade-plan.json
cellc update . --offline --output target/upgrade-plan.json
cellc update-plan . --package math --precise 2.4.1 --output target/math-upgrade.json
cellc package update-plan . --environment testnet --json
```

Planning requires an existing authoritative lock for a standalone package and
for workspace members with dependencies. A dependency-free workspace member
without a lock is not synthesized into the transaction. It does not modify a lock, `Deployed.toml`, build output, or
cache recency. `--offline` also forbids external resolution and uses only
verified local source caches. Omitting it permits the configured update-time
Registry or Git resolver to find a candidate; the resulting lock still records
an immutable source identity.

The default scope includes runtime and test dependencies and activates all
declared feature roots, matching a complete package lock. Use `--scope runtime`,
`--features`, `--all-features`, `--no-default-features`, or `--environment` to
make a narrower selection explicit. `--package` limits replacement to one
dependency alias or package coordinate and its required descendant closure;
unrelated nodes retain their exact old records. `--precise` requires
`--package`, accepts an exact SemVer within the manifest requirement, and is
available for direct Registry or configured external-resolver dependencies.
It cannot substitute an immutable path or Git dependency.

## Review The Receipt

The plan binds its entire canonical JSON body with `plan_hash`, records the
active compiler version, and names the `cellscript-resolve-graph-v1` and
`cellscript-build-plan-v2` contracts it composes. Every affected member has:

- exact old and candidate lock text, parsed lock records, and SHA-256 hashes;
- aggregate old and candidate graph hashes;
- package-node changes classified as added, removed, upgraded, downgraded,
  source-switched, feature/environment-changed, or content-changed;
- edge additions, removals, and retargeting with owner and alias provenance;
- old and candidate build-unit identities plus reverse-dependent compilation
  evidence for every selected CKB environment;
- source API, serialized layout, runtime ABI, effects/capabilities, builder,
  and deployment compatibility as six independent interface dimensions;
- typed-semantics, builder-contract, deployment-contract, and ProtocolBundle
  input identities;
- deployment upgrade status and a separate authorization result.

The planner compiles both graphs through in-memory lock overrides. A candidate
that does not compile is a hard blocker. If the old graph can no longer be
compiled, `UPG3001` is a hard blocker: schema review requires its actual
source-bound graph. Retained build metadata cannot replace that source.
Breaking interface dimensions use `UPG3101` through `UPG3106` and list the migration artifacts needed for review.

Dependency downgrades (`UPG2001`), source-authority substitutions (`UPG2002`),
and feature or environment selection changes (`UPG2003`) are separate policy
events. Deployment checks also stay separate: an immutable deployment has no
upgrade authority (`UPG4001`), malformed TYPE_ID lineage is rejected
(`UPG4002`), and valid local TYPE_ID metadata still requires an external live
authorization proof (`UPG4003`). Acknowledging one of these codes records a
review decision; it does not create a state migration, deployment transaction,
signature, or on-chain authorization.

## Review Schema Changes

The v2 plan also binds root `source_baselines`, `dependency_schema_impacts`,
and per-subject/environment `schema_reviews`. Changed dependency packages are
reviewed by their own source modules, including library-only packages and
actions that no root currently calls. Their sources come from the package file
set verified by the consuming lock graph; review does not require a dependency's
standalone lock or a default application entry. Exact old/new graph, module and
compilation-profile identities separate their contexts. A dependency can have
multiple module impacts; equivalent reviews share one subject with explicit
root owners.
Each review identifies the old/new resource and relation, complete field-policy
changes, blockers, and its optional verified receipt.
Old and candidate source graphs are compiled separately; the current source
must not be used as evidence of the old layout or relation.

Schema review covers the union of old and new manifest environment scopes.
`--environment` limits version repinning, not that review coverage. Each impact
records an `environment_transition` distinguishing existing, introduced, retired,
or unavailable-old-baseline contexts. Evidence explicitly marks an absent new
or retired side; genuinely unavailable old evidence remains a hard blocker.
Deleting or renaming an environment therefore cannot discard its previous
`same except` relations or bypass their required review. Apply rebuilds the
same environment union before writing locks.

A source namespace change retains separate retired old-module and fresh
new-module review endpoints. Their `old_module`/`new_module` fields name the
actual sides, and both records bind the old/new source identities and exact graph
context. The retired `same except` relation still needs its required receipt and
state migration decision; the planner does not infer nominal continuity across
the renamed module. Use `schema-ack --upgrade-plan ... --subject ... --module
OLD_MODULE ...` to select a retired endpoint when action and role names overlap.
Standalone `schema-ack --old ... --new ...` requires matching module names.

If a path dependency or the root source changes, preserve its old package tree
before editing and supply `--schema-baseline MEMBER=PATH`. The baseline source
hash and exact lock bytes must agree with the authoritative old lock. Preserve
relative dependency paths in that old tree. For example:

```bash
cellc update-plan . --offline \
  --schema-baseline app=../old-tree/app \
  --output target/upgrade-plan.json
```

An added field kept implicit by `same except` remains a `SACK1001` blocker.
State the intended field treatment explicitly, then create a receipt with
`cellc schema-ack --upgrade-plan ... --subject ... --action ... --before ...
--after ...`. Attach it when regenerating the plan through
`--schema-ack SUBJECT=FILE` or `--schema-ack SUBJECT@ENVIRONMENT=FILE`. The
[schema review workflow](CELLSCRIPT_SCHEMA_ACKNOWLEDGEMENTS.md#transactional-upgrade-integration)
has complete commands.

Every required review must be covered. Missing or stale receipts and missing
old source are hard blockers, including when generic acknowledgement codes are
supplied. A genuinely new resource baseline has no old-to-new receipt; that rule
does not exempt an upgrade whose old source is unavailable. A reviewed schema
change still requires its own state migration and deployment authorization.

## Apply A Reviewed Plan

```bash
cellc update --apply-plan target/upgrade-plan.json
cellc update --apply-plan target/upgrade-plan.json \
  --acknowledge UPG2003,UPG3102
```

Apply revalidates the schema, active compiler, plan hash, required
acknowledgements, exact old-source baselines and schema-review receipts, every
old lock byte sequence, and every candidate lock hash and canonical TOML
representation. Any changed old lock makes the plan
stale and aborts the whole preflight. Paths must remain confined to the planned
workspace and may not be symlinks.

After all members pass preflight, each lock is replaced through a synced
same-directory temporary file and rename. If a later replacement fails during
the operation, already replaced locks are restored from the exact old bytes.
This provides a preflighted multi-member transaction with atomic per-file
replacement and runtime rollback. It is not a cross-filesystem crash-atomic
journal.

Apply changes only the listed `Cell.lock` files. It never edits
`Deployed.toml`, deploys or publishes an artifact, signs a transaction, proves
TYPE_ID authority, or performs state migration. Those actions require their
own reviewed artifacts and release or deployment gates.

## Stable Fail-Closed Diagnostics

| Code | Meaning |
| --- | --- |
| `E2800` | Invalid plan request, selector, schema negotiation, or candidate graph |
| `E2801` | Apply is blocked by missing acknowledgement or a hard diagnostic |
| `E2802` | Plan schema/compiler/hash/path/candidate validation failed, or the old lock is stale |
| `UPG2001`–`UPG2003` | Dependency downgrade, source switch, or selection change |
| `UPG3000`–`UPG3002` | Reverse-dependent compile or interface-evidence failure |
| `UPG3200`–`UPG3201` | Invalid/reconstructed schema review or missing required schema acknowledgement |
| `SACK1001` | Newly added field remains implicitly preserved |
| `UPG3101`–`UPG3106` | Breaking API, layout, ABI, effects, builder, or deployment dimension |
| `UPG4001`–`UPG4003` | Missing, malformed, or externally unproven deployment upgrade authority |

Unknown plan fields and unsupported schema versions fail closed. The v2 JSON
fixture is `tests/fixtures/upgrade_plan_v2.json`; routine gates also generate an
offline plan and prove that its source `Cell.lock` remains byte-identical.
