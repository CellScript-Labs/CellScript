# Myelin Handoff Contract

`integrations/myelin/cellscript-handoff-contract.json` records the current
compiler, Edition 2027 source profile, metadata 75, lowering/typed semantics v12
and source-map v4 boundary. The external adoption state is pending an exact
release pin. Source, binary, artifact and checker digests must be supplied by
an actual build; this file is a contract, not an adoption attestation.

The compiler emits untrusted access templates. Myelin owns authenticated concrete
Cell resolution, scheduler sidecars and transaction identity binding. The CKB
runtime and canonical witness placement stay explicit. Admission requires exact
source/artifact/checker bindings and the receiving system's production gate.
