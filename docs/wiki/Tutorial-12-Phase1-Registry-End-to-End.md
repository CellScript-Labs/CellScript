# Phase 1 Registry: End-to-End

This page is a **navigation stub** for the Phase 1 registry
end-to-end tutorial. The full tutorial lives in the repository under
`docs/tutorials/phase1-end-to-end.md` and is the canonical source for
this material.

## Read the canonical tutorial

[Phase 1 Registry: End-to-End Tutorial](../tutorials/phase1-end-to-end.md)

## What this page is

The wiki hosts short, reader-facing navigation. The Phase 1 tutorial
is long enough that keeping the full text here would drift out of
sync with the canonical repository copy. This page exists so a wiki
reader can find the tutorial through the wiki sidebar without having
to navigate the repository layout directly.

If you are reading this in the wiki and the canonical tutorial is
inaccessible, the tutorial covers:

- the three-file identity model (manifest, build lockfile, deployment
  manifest);
- the three-layer trust model (source identity, build identity,
  deployment identity) and its fail-closed semantics at each layer;
- how the registry separates authenticated publish writes from static,
  cacheable reads, and how private/offline mirrors can still operate without
  GitHub;
- a worked end-to-end example that walks authoring, publishing,
  dependency resolution, building, deployment, and verification.

## See also

- [Packages and CLI Workflow](Tutorial-04-Packages-and-CLI-Workflow)
  — the wiki overview of the package tooling surface that Phase 1
  identity rides on top of.
- [Metadata, Verification, and Production Gates](Tutorial-06-Metadata-Verification-and-Production-Gates)
  — the wiki overview of the verification and gate surface that
  consumes Phase 1 identity records.
- [CKB Target Profiles](Tutorial-05-CKB-Target-Profiles) — the wiki
  overview of the target profile boundary that Phase 1 deployment
  records reference.
- `docs/CELLSCRIPT_PACKAGE_PROVENANCE_AND_DEPLOYMENT_IDENTITY.md` —
  the in-repository reference design for Phase 1 identity.
- `docs/CELLSCRIPT_REGISTRY_PHASE1.md` — the in-repository Phase 1
  user-facing walkthrough.
