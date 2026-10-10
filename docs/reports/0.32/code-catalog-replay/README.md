# Frozen code catalog replay archive

The manifest binds clean full backend, strict audit, CKB acceptance and local
Counter/CCC evidence to G-signed `16e6b721`. All three CCC rows have actual fee
signatures and confirmations. The later G-signed `04a70f61` delta changes native
source ownership only; its own staged dev and push logs are archived separately,
without claiming a fresh full backend on that later commit.

The archived dev logs bind exact staged source files later signed at the manifest's
listed source commits; they are not described as clean-HEAD dev runs. Each gzip
file records raw/stored lengths and SHA-256 and uses a normalized zero timestamp.
The containing archive commit does not replace the recorded source provenance.

These are finite host prerequisites and existing private local-network Counter
replays. Production/public deployment admission, complete H1/H2, #29 and
#44–#46 are not established. Full CI remains deferred until the whole
implementation queue is ready; independent security review remains unassigned.

Parsed-source capture passed staged dev and was G-signed/pushed as
`3a15ed9a0a462c089845bff6eb08993dd9221deb`. The additional source dev/push
logs retain that native-only provenance; they do not update the full backend
or CCC source commit. All twenty-seven gzip files have raw/stored sizes and SHA-256
in the manifest. No later containing commit replaces any evidence source commit.

Target selection passed the refreshed staged dev and was G-signed/pushed as
`35a9565b260890e2b32f6082af51e5cfa36b3866`. Its own dev/push logs retain this
native/checker provenance. The earlier stale-inventory dev failure is not
archived as a passing run. Clean backend/CCC provenance
remains exactly `16e6b721`.

Finite policy receipts passed the frozen staged dev, were G-signed as
`e546d0f93dafc52a2e522f80b7422a7f88307a34` and pushed to public `0.32`.
Their dev, push and independent remote-head lookup are archived separately.
All twenty-seven raw/stored lengths and SHA-256 were verified. Clean full backend
and CCC still bind exactly `16e6b721`; no later host-only delta substitutes its
source identity into those earlier reports.

All-member finite policy bindings passed frozen staged dev and were G-signed as
`f6ef9711ece01738794ff6e7c8288faa63a7fc29`, pushed and independently confirmed
at public `0.32`. Its dev/push/remote logs retain native host-only provenance.
All twenty-seven raw/stored lengths and SHA-256 were verified. The current source
receipt/version work is a separate slice; these logs do not validate that later
uncommitted source or replace clean backend/CCC source `16e6b721`.


Source receipt/version policies passed frozen staged dev and were G-signed as
`d424583ad7cff22dd4f11826fdfa5f0c015e5aa4`, pushed and independently confirmed
at public `0.32`. Their dev/push/remote logs retain native host-only provenance.
All twenty-seven raw/stored lengths and SHA-256 were verified. The subsequent
direct-dependency/typed-source-proof work is a separate slice, not validated by
these earlier logs. Clean full backend/CCC still bind exactly `16e6b721`.


Direct dependency snapshots and distinct source proof types passed frozen staged
dev and were G-signed as `0b187a7f3045c02cf1faca43b0aead7439965d7b`,
pushed and independently confirmed at public `0.32`. Their three logs retain
that host-only provenance; all twenty-seven raw/stored lengths and SHA-256 were
verified. The subsequent complete Type-group/witness snapshot is a separate
slice. Clean full backend/CCC still bind exactly `16e6b721`.
