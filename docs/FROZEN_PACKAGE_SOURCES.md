# Parsed package sources before consumer typing

Native `freeze_package_sources(root, environment)` captures an actual parsed,
lock-pinned source closure without compiling a consumer ELF. It reuses the
existing native frozen-module preflight: offline authoritative `Cell.lock`,
bounded regular source/manifest files, 4 MiB/file, 16 MiB/source capture,
32 packages, 256 modules and pinned chain/genesis. The private
`FrozenPackageSources` stores source hashes/context and private path locators,
not all source file contents. Exported context JSON cannot construct it.

This addresses a sequencing prerequisite for future nominal interface typing:
the actual defining source owner must be known before new consumer types can
be checked. Capture parses source; unknown semantic types still fail subsequent
compilation. It does not introduce or certify `I`, `ScriptHandle<I>` or
`VerifierHandle<I>`, nor a typed/checked consumer artifact.

`resolve_source_catalog` consumes that snapshot and the actual finite checked
`FrozenCodeCatalog`, verifies the actual files remain unchanged, then reuses the
same defining-owner/closure matcher as compiled-consumer source binding. Owner
identity stays `cellscript-resolver-interface-source-owner-v1`, with the same
hash domain and actual defining-closure package identities. A separate
`cellscript-resolver-source-catalog-v1` record/domain
`cellscript-resolver-source-catalog-id-v1` distinguishes this pre-typing binding
from the compiled-consumer binding. Source capture and actual code-catalog
checking retain their separately bounded 16 MiB phases; this API does not claim
one combined source-plus-code preparse budget or a consumer four-file tuple.

`check_unchanged` / `check_unchanged_sources` reread the same pinned real sources
offline without repinning or writing a lock. Source/manifest/lock/path changes
reject. Both initial capture and every later recheck reject upgrade-planning
lockfile overrides, even if the planned graph equals the real graph. Future
compiler integration must check the snapshot before accepting a type context
and after compilation; one earlier check does not guarantee later filesystem
state.

Tests compare parsed source context against actual O0–O3 native compilation,
preserve lock bytes, reject changes before ownership binding and after explicit
repinning, and preserve the same defining owner before consumer typing. An
unknown `FutureHandle` can be parsed and source-bound but remains a compiler
type error. Missing locks/environments, malformed source, oversized input and
identical planned overrides reject. The existing compiled-owner integration
tests still exercise the shared matcher.

This is host source ownership, not complete H1/H2, source-to-machine/behavior
equivalence, live deployment, Type replacement history, immutable authority,
ProtocolBundle signing admission or peer execution. #29 and #44–#46 remain
unfinished. Independent review remains required before stable admission, and
full CI remains deferred until the complete implementation queue is ready.
