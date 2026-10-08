# Clean-source CCC application and migration replay

The complete `backend` gate passed on signed commit
`bfb1910070d90a15e6268796a0e329e584806305`, with
`CELLSCRIPT_COUNTER_CCC=1` and the pinned CKB checkout at
`f7fa4436737756f97a24e254f22c13a36316ecea`. The complete `dev` gate passed before
that commit. Full CI is deferred until the maintainer's ordered implementation
queue is complete; this record does not claim current CI or release admission.

[manifest.json](manifest.json) binds raw and stored sizes/SHA-256 digests of the
complete backend log, fresh clean-source CKB acceptance receipt, node evidence,
and both CCC client reports. The node test independently requires the acceptance
receipt's source commit to equal its current clean checkout. These are fresh
CCC runs; the earlier [resumed record](../resumed/README.md) retains its separate
default-node-only scope.

The ordinary CCC workflow confirms two consecutive updates, rejects transaction,
proof, dependency, network and pinned-prover substitutions, and exercises
interrupted-proving recovery and stale-input rejection. The
[migration report](ccc-migration.json) confirms this exact continuous lineage:

| Transaction | Selected version | CKB cycles | Full transaction bytes |
| --- | --- | ---: | ---: |
| Old-key update | 0 | 111,666,202 | 1,237 |
| Old-key-authorized migration | 0 | 111,907,727 | 1,632 |
| New-key successor update | 1 | 111,828,719 | 1,237 |

All three transactions have actual fee signatures and ordinary node admission
and confirmation. The client checks actual successor continuity and retained
instance/configuration identity. New-key self-installation, old-key use after
migration, a second migration, stale configuration and raw-transaction mutation
reject at their declared checks. The existing immutable legacy application
remains non-migratable; this is the separately initialized paired migratable
application's first authorized migration.

Both versions use explicitly labeled **public test setup seeds**. This record
does not establish production setup trust, application privacy, a public-network
deployment, generic circuit/VK migration, multi-Cell acceptance or an independent
security review. The frozen first-profile budgets and the #40 cost corpus are
unchanged. Per-VM stack/resource evidence remains in the separate resource replay;
the table does not infer host proving memory or sum independent VM stack peaks.

#42's P1 implementation now has current-source application and migration
execution evidence. Its P2/P3/P4 follow-ups remain #44/#45/#46; their implementations
and the whole queue's final CI are still pending.
