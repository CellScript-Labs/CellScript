# Frozen code catalog replay archive

The manifest binds clean full backend, strict audit, CKB acceptance and local
Counter/CCC evidence to G-signed `16e6b721`. All three CCC rows have actual fee
signatures and confirmations. The later G-signed `04a70f61` delta changes native
source ownership only; its own staged dev and push logs are archived separately,
without claiming a fresh full backend on that later commit.

The four dev logs bind exact staged source files later signed at the manifest's
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
or CCC source commit. All thirteen gzip files have raw/stored sizes and SHA-256
in the manifest. No later containing commit replaces any evidence source commit.
