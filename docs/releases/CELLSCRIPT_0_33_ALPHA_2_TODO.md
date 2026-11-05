# CellScript 0.33.0-alpha.2 follow-up

Updated: 2026-11-05. This backlog covers unfinished work on the
[sole Edition 2027 source contract](../CELLSCRIPT_SOURCE_CONTRACT.md).
The [release notes](CELLSCRIPT_0_33_ALPHA_2_RELEASE_NOTES.md) describe the current
implementation and its validation scope. This backlog does not authorize
publication, deployment, version changes or external messages.

## Current foundation

Native/package/LSP/WASM entry points and ecosystem consumers use the same
current contract: metadata 75, lowering records v12, typed semantics v12,
semantic foundation v4 and package interface v6. Readers require these shapes;
operation/disposition records and `implicit-public` use the current vocabulary.
Deployment and builder schemas are edition-neutral. Package locks, generated
builders, browser/editor assets, Registry readers and proposal consumers have
been updated together.

The commit, dev and CI gates passed on this compiler/checker implementation.
Backend and real-node evidence must be read from the matching source-commit
receipts; another gate or a component test does not substitute for them.
Implementation checkpoints and historical acceptance runs do not close the
open representation or proof obligations below.

## A2-04 / #96: complete structured type identity

Owner: frontend, type-system and IR maintainers, with independent-checker and
interface-consumer review.

- [ ] Complete the remaining nominal/application representation and
  Cell/selection family work under the
  [structured identity contract](../CELLSCRIPT_STRUCTURED_TYPE_IDENTITY_RFC.md).
  Preserve defining owners, argument identities and source diagnostic spelling
  through resolution, specialization, lowering and interface emission.
- [ ] Supply sufficient independently checked alias evidence in ordinary
  compilation. An optional declaration-emission flag must not be the permanent
  requirement for validating an otherwise admitted source program.
- [ ] Keep reserved `ExactCellSelection` and unsupported open nested-view
  projections outside executable admission until their ownership, layout,
  source and machine contracts are specified and tested together.

Acceptance: compiler, native/WASM metadata, LSP, interface consumers and the
standalone checker agree on admitted identities. Cross-owner same-name and
same-width substitutions reject after all outer hashes are rebound. Positive
source/ELF/VM cases preserve their intended behavior across optimization modes.
Close #96 only after the complete scoped contract is implemented and reviewed.

## A2-06 / #97: finish diagnostic identities at phase boundaries

Owner: compiler phase owners and language-service maintainers.

- [ ] Classify the remaining uncoded input failures and message-based consumers;
  move each affected boundary to a stable diagnostic identity without changing
  the standalone checker's ownership of artifact rejection codes.

Acceptance: malformed or unsupported source returns a located diagnostic through
native compilation, metadata compilation and editor analysis. Changed consumers
assert the diagnostic identity; valid controls still compile. A search count or
large module is not evidence of a defect.

## A2-08 / #85: review structured effect-fact closure

Owner: semantic-analysis, independent-checker, scheduler and consumer maintainers.

- [ ] Close the issue only after reviewing the complete inferred fact contract,
  independent reconstruction, interface comparison and all affected consumers
  against identified source and its applicable gate receipts.

Acceptance: context reads, lifecycle operations and process operations remain
separate facts; unknown operations, substituted declarations and widened facts
cannot obtain stronger checked evidence through hash rebinding. Issue closure
must reflect the implemented whole flow rather than an intermediate checkpoint.

## Runtime and production proof boundaries

Owner: runtime/codegen and standalone-checker maintainers; application/proposal
owners retain responsibility for their own production claims.

- [ ] Specify and implement any proposed expansion of dynamic-element vector,
  open-view or deferred runtime semantics before admitting it. Until then,
  retain the explicit fail-closed boundary and its production-policy rejection.
- [ ] Keep finite ABI recognizers and bounded machine checks explicit about what
  they establish. New claims require positive execution, hostile decoded-machine
  mutations and independently checked source/artifact linkage.
- [ ] Require application-specific transaction, deployment and external verifier
  evidence before a production claim. Research protocol equivalence remains
  `NOT_PROVEN`; selected local scenarios do not establish general equivalence.

Acceptance: the owning language/runtime contract, generated code, metadata,
checker, examples and consumer behavior agree. CKB stateful claims identify the
clean source commit, pinned node, deployed code and complete required scenario
coverage. Proposal-local certificates retain their stated scope.

## A2-14: final evidence and external distribution

Owner: release maintainer, with the owners of each gate and distribution.

- Bind integration and stateful claims to the final clean source commit and
  matching backend/production receipts. Preserve failed or interrupted receipts
  with their actual scope; a skipped or deferred check is not a pass.
- Satisfy the documented release requirements before claiming full release
  acceptance. Local commit/dev/CI results do not imply a `release` gate result.
- Require every release attachment, its schemas, checksums and validation
  receipts to identify the same reviewed source. A GitHub attachment update
  does not establish acceptance of another distribution.
- Keep crates.io publication, website deployment and Marketplace publication
  outside this source-contract refresh; each requires its own recorded action
  and distribution evidence.

Acceptance: the [gate policy](../CELLSCRIPT_GATE_POLICY.md), release body,
attached receipts and distributed bytes identify the same reviewed source and
state their exclusions accurately. No historical publication exception carries
authority for a new release action.
