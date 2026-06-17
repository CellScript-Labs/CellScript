# Production Readiness Gate

DOB-EVO/1 is written as a production profile, not as a compatibility prototype.
The local source/package gate is:

```bash
cellc build --release --target riscv64-elf --target-profile ckb
cellc check --target-profile ckb --primitive-strict 0.16
cellc package verify --json
cellc publish --dry-run
python3 scripts/evolving_dob_registry_pressure.py
```

The manifest requires `production = true`, `deny_fail_closed = true`, and
`deny_runtime_obligations = true`. `deny_ckb_runtime` remains false because
DOB-EVO/1 is a CKB type-script package and must use CKB runtime syscalls to
verify inputs, outputs, lock hashes, and TYPE_ID lifecycle.

Strict local CKB devnet workflow:

```bash
python3 scripts/evolving_dob_devnet_workflow.py --pretty
```

The devnet workflow deploys the compiled ELF into a local CKB integration node,
records the live outpoint in `Deployed.toml`, rebuilds `Cell.lock`, runs:

```bash
cellc registry verify --json --require-publisher-signature --require-audit-report
cellc registry verify --live --rpc-url "$LOCAL_CKB_RPC" --network devnet --json \
  --require-publisher-signature --require-audit-report
cellc gen-builder --target typescript --lockfile Cell.lock --deployed Deployed.toml \
  --deployment-network devnet
npm --prefix target/devnet-workflow/.../generated-builder test
```

For public devnet or mainnet promotion, replace the local integration-node
facts with a `Deployed.toml` generated from the actual chain deployment and run
the same `registry verify --live` gate against that RPC endpoint. The package
must not be presented as deployed public infrastructure until that public-chain
live verification passes against the locked build identity.
