# Fixed external scalar VM evidence

The raw focused log and all 40 runtime rows come from the actual qualified
VM test. Inner args are checked against literal byte arrays; wrong values and
truncated/appended inputs reject. Source SHA-256 entries bind the exact staged
sources, after signed `588f6a28`; the containing commit is created later. This
run used a dirty checkout and is not clean backend evidence. Normalized gzip
storage has independent raw and stored digests.

These are CKB-VM cycles/ELF/inner-args sizes for a finite policy fixture, including
zero/nonzero absolute group positions. They are not host/prover benchmarks,
full transaction costs, deployment/history authority, source behavior equivalence,
complete H1/H2 or stable admission. The existing 30-million-cycle test ceiling is
unchanged. Reproduce through the qualified test or the dev gate.
