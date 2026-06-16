# Phase 1 Registry: End-to-End Tutorial

This tutorial walks a package author through the full Phase 1 identity
loop — authoring, packaging, publishing, dependency resolution,
build, deployment, and verification — without requiring a running
chain or a private server.

By the end of this tutorial you will understand:

- the three-file identity model that powers Phase 1;
- how source identity is bound to a built artifact and then to an
  on-chain deployment record;
- why Phase 1 does not need a registry server, and what it actually
  relies on;
- how a verifier downstream of you can confirm that what they
  imported, compiled, and deployed is the same thing you published.

## Audience

You are writing or porting a contract for CKB. You have a working
toolchain that compiles your language source into a RISC-V binary
targeting the CKB VM. You want to publish the contract so other
people can pull it as a dependency, lock its build identity, and
verify it against on-chain deployment records — all without
registering with a central service.

## What Phase 1 is and is not

Phase 1 is a deliberately minimal registry layer. It answers three
questions and refuses to answer a fourth:

| Question | How Phase 1 answers it |
|---|---|
| "Where does the source live?" | A Git URL plus a content-addressed source hash. |
| "What did the toolchain produce?" | Six build hashes (artifact, metadata, schema, ABI, constraints, compiler/version + target profile) recorded next to the source identity. |
| "Where is that build on a chain?" | A deployment record with the chain id, transaction hash, output index, code hash, data hash, and out point. |

It does **not** answer "is the chain still live and does the cell match
today" — that is the next layer's job, and Phase 1 is designed to
compose with it rather than replace it.

Phase 1 also refuses to be a substitute for chain acceptance. A
deployment record that names the right cell hash is not the same as
a cell that the CKB VM will accept today. Use Phase 1 to bind
identity; use the chain to verify behavior.

## The three files

You will see three files appear during this tutorial. They each carry
one slice of identity, and they form a fail-closed chain when verified
together.

### The package manifest

The package manifest declares your intent: package name, version,
namespace, source roots, dependency declarations, and build
configuration. It is the only file you edit by hand. The toolchain
generates a starter on `init` and you evolve it from there.

A manifest typically carries:

- a single entry pointing at the language source you author;
- a list of dependencies, each with a version range and a source kind
  (`registry`, `git`, or `path`);
- the toolchain version and target profile that the package compiles
  against;
- an optional policy block for release-grade gates (fail-closed
  invariants, runtime obligations, etc.).

The manifest is the **only** file that names a version. Every other
identity record derives from a manifest plus the source tree plus a
specific build run.

### The build lockfile

The lockfile is written automatically every time the toolchain
finishes a successful compile. It records the **current** build
identity so the next build, the verifier, and the deploy adapter can
all reason about the same artifact.

A lockfile entry covers:

- the source hash of the package (so a tampered source tree fails the
  next build);
- the six build hashes from the most recent compile;
- the compiler version and target profile (so swapping the toolchain
  is itself an identity break);
- per-network deployment references that the deploy adapter has
  recorded since the last build.

You do not edit the lockfile. You delete it to force a clean rebuild.

### The deployment manifest

The deployment manifest records where the build actually landed on a
chain. The deploy adapter writes it after a successful on-chain
submission. It is the bridge between the off-chain build identity
and the on-chain cell identity.

A deployment record covers:

- the network name (a free-text label, e.g. `devnet`, `mainnet`, or
  an internal staging net);
- the chain id, transaction hash, and output index;
- the code hash, hash type, and data hash that identify the deployed
  cell;
- the matching subset of the build hashes, so a verifier can prove
  that the cell on chain corresponds to the lockfile build, not to
  some other artifact with the same code hash.

When the deploy adapter writes a deployment record, it also tells the
next build run to remember that deployment in the lockfile, so the
verifier does not have to re-discover it.

## The three-layer trust model

Layer 1 — source identity — protects against source tampering.
Anyone who clones your package and rebuilds it computes the same
source hash. If they get a different hash, they know the source tree
diverged from the version they trusted.

Layer 2 — build identity — protects against build substitution.
Two builds of the same source with different toolchains, different
optimization levels, or different targets all produce different build
hashes. The lockfile pins the build that your downstream consumers
are expected to redeploy.

Layer 3 — deployment identity — protects against cell substitution.
A verifier checks that the cell at the recorded out point has the
recorded code hash and data hash, and that the recorded chain id
matches the actual chain the verifier is talking to. If any of those
four things disagree, the deployment record is wrong.

Phase 1's verification is **fail-closed at every layer**. A missing
hash, an empty record, or a cross-layer disagreement is a verification
failure. There is no "best effort" mode that lets a partial match
through.

## Why Phase 1 has no server

Most package registries need a server because they need to mediate
between publishers and consumers. Phase 1 does not need that
mediation:

- The source already lives somewhere with content addressing
  (Git, with commit hashes).
- The build hash is computed locally from the source and the
  toolchain.
- The deployment record is a small text file with chain facts that
  the verifier can re-derive from the chain itself.

This means there is no canonical index you must register with. You
push the source to any Git host. Consumers clone from the same host,
recompute the source hash, and proceed. There is no "yank"
equivalent — once a version is out, the only way to replace it is
to publish a new version that downstream users opt into.

The one piece of optional state is a tiny **discovery index**: a
small JSON file per package that maps a `(namespace, name)` pair to a
Git URL. The index exists only to support packages whose source
location does not follow the default convention. The default
convention is: a package named `cellscript/foo` lives at
`github.com/cellscript/foo`. If you follow the convention, you never
touch the discovery index.

## Authoring a package from scratch

The authoring flow has four steps and ends with a published version
that other people can pull.

### Step 1 — Create a manifest and source layout

Run the toolchain's `init` command in an empty directory. It creates
a manifest, a starter source file, a tests directory, and an examples
directory. The starter source already contains a minimal resource
declaration so the very first build produces a valid binary.

Open the manifest and fill in the metadata your consumers will see:
namespace, version, description, license, repository URL. Leave the
entry point alone until you have written the source it should
compile.

### Step 2 — Author the source

Write the contract source in your language's syntax, using the
CellScript grammar for resource declarations, action definitions,
and lock scripts. The starter source shows the minimum shape.

Build incrementally as you go. Each successful build updates the
lockfile with the build identity that downstream consumers will
later lock against. You do not need to write the lockfile yourself.

### Step 3 — Pin dependencies

If your contract imports another contract, declare it in the
manifest's dependency section. There are three dependency kinds:

- a **registry** dependency, named by `(namespace, name)` plus a
  version range. The toolchain resolves it via the discovery index
  when present, or via the default convention when not.
- a **git** dependency, named by a Git URL plus an optional tag,
  branch, or revision. The toolchain clones the URL into a local
  cache.
- a **path** dependency, named by a relative or absolute path on
  the local filesystem. The toolchain reads it directly.

When the toolchain resolves a dependency, it also records the
**source hash** of the resolved source. If the upstream later changes
that source without bumping the version, the next build fails closed
because the recorded hash no longer matches the source on disk.

### Step 4 — Publish a version

Run the toolchain's `publish` command. It does three things:

1. Recomputes the source hash from the current working tree and the
   current manifest, so the published hash matches the source you
   actually pushed.
2. Recomputes the build hash from the current built artifact, so the
   published build identity matches the binary you actually built.
3. Appends a version entry to a small metadata file inside the
   source repo, then writes nothing else.

After publish, commit the metadata file, tag the commit, and push
both. The metadata file travels with the source: every clone of your
repo at that tag will reproduce the same source hash and the same
build hash.

You can publish as many versions as you want. Each version is a new
metadata entry plus a new tag. There is no global namespace server
that has to approve the version.

## Consuming a package as a dependency

The consuming flow is the mirror of the authoring flow.

### Step 1 — Declare the dependency

Add a registry, git, or path entry to your consumer manifest's
dependency section. The toolchain validates the kind: registry
dependencies take namespace and name, git dependencies take a URL,
path dependencies take a filesystem path.

### Step 2 — Resolve

Run the toolchain's resolver. It performs three checks per
dependency:

1. It fetches the source through the right channel (discovery
   index, Git clone, or local read).
2. It computes the source hash and compares it to the recorded
   source hash from the metadata. A mismatch is a hard error.
3. It transitively resolves any dependencies that the dependency
   itself declares.

The resolver writes a snapshot of the resolved graph into the
lockfile so subsequent builds do not need network access.

### Step 3 — Build

Run the build. The build reads the lockfile, recomputes the source
hash against the resolved source tree, compiles, computes the six
build hashes, and writes the updated lockfile.

A build fails if any of the following is true:

- the source hash in the lockfile no longer matches the resolved
  source tree;
- a transitive dependency failed to resolve;
- the compiler version changed;
- the target profile changed;
- the source no longer compiles under the locked policy.

### Step 4 — Verify

Run the verifier. It reads the lockfile and the manifest and reports
whether every declared hash is present and consistent. Verification
is a single command and produces a structured report so it can be
wired into CI.

## Deployment and verification

The deployment flow binds an off-chain build to an on-chain cell.

### Step 1 — Deploy with the adapter

The deploy adapter takes a built artifact, a network name, and a
chain RPC URL. It submits the artifact as a code cell on the
target chain, optionally with a Type ID or other upgrade lineage
metadata. On success, the adapter writes a deployment manifest next
to the lockfile.

### Step 2 — Refresh the lockfile

The next build run notices the new deployment manifest and copies the
deployment references into the lockfile under per-network keys. This
is what makes verification downstream cheap: the lockfile already
knows where each network's deployment lives.

### Step 3 — Verify locally

Run the verifier against the lockfile and the deployment manifest.
The verifier checks:

- the package source hash matches across manifest, lockfile, and
  deployment manifest;
- the six build hashes match between the lockfile and the deployment
  record;
- the deployment record has the expected network name, chain id,
  and status.

A local verification does not touch the chain. It is fast and
deterministic and can run in CI without any RPC credentials.

### Step 4 — Verify on chain

The verifier can optionally call a chain RPC. With the
`--require-live-evidence` flag, the verifier also:

- fetches the block chain info and compares the chain id to the
  recorded chain id;
- fetches the live cell at the recorded out point and compares its
  code hash and data hash to the recorded values.

A chain verification that disagrees with the recorded deployment is
a hard failure. Phase 1 does not silently override stale records
with chain facts — it surfaces the disagreement and lets you decide
which side is wrong.

## Failure modes the system catches for you

The combination of three fail-closed layers means the most common
mistakes cannot reach a verifier without being noticed.

| Mistake | How Phase 1 catches it |
|---|---|
| Source was edited after lockfile was written | Source hash mismatch on the next build. |
| Toolchain was upgraded without bumping version | Compiler version field in the lockfile disagrees with the manifest's declared toolchain. |
| Dependency was force-pushed upstream | Source hash mismatch when the consumer next resolves. |
| Cell on chain was replaced by a different cell with the same code hash | Data hash mismatch on live verification, or build hash mismatch on local verification. |
| Deployment was recorded against the wrong chain | Chain id mismatch on live verification. |
| Deployment manifest was edited by hand | Build hash fields that the adapter would have filled disagree with the lockfile, and the next build refuses to refresh the lockfile from a tampered deployment manifest. |

## What Phase 1 deliberately does not do

Knowing what is out of scope is as important as knowing what is in
scope.

- **No on-chain registry.** The chain stores cells, not package
  indexes. Any on-chain registry or proxy is a separate, optional
  layer.
- **No publisher signatures in the registry record.** Phase 1 binds
  hashes, not identities. Signature-based trust belongs to a later
  trust-hardening layer and composes with Phase 1 rather than
  replacing it.
- **No mutable channels (`latest`, `stable`).** Versions are
  pinned by tag and by source hash. There is no shortcut that
  silently upgrades a dependency.
- **No automatic cross-platform reuse.** Source identity is
  per-toolchain-per-target-profile. If you publish the same source
  for two profiles, you publish two version entries.
- **No silent re-verification.** Every verifier run is a fresh check.
  There is no "last known good" cache.

## Operating without GitHub

Phase 1 has no dependency on GitHub. The discovery index is a tiny
JSON file that lives anywhere you want it to live. The package
metadata lives inside the source repo and travels with it. The
resolver clones from any Git URL it can reach, including self-hosted
Gitea, GitLab, or a bare repo on a file share.

For air-gapped environments, declare dependencies as `path` or as
`git` URLs pointing at a local mirror. The lockfile pins the
resolved source hash, so subsequent builds need no network at all
as long as the local cache still contains the pinned commit.

For private registries inside an organization, run a local
discovery index as a flat directory of JSON files. No daemon, no
database, no authentication server. The resolver reads it like
any other static file.

## How this fits with the rest of the system

Phase 1 is the identity layer. It sits between your source and the
chain. Layers above it consume the identity it produces:

- a **builder** tool takes a manifest plus a lockfile plus a
  deployment manifest and emits a typed builder package for one
  action, with no further chain semantics of its own;
- a **stateful flow runner** consumes multiple builders and chains
  their transactions in order, using the same identity chain to
  verify each step;
- an **on-chain verifier** consumes the lockfile plus a live RPC
  and proves that the chain still agrees with the record.

All of these layers consume Phase 1 output as input. None of them
replace it. That is the point of keeping the registry layer small —
it can be trusted because it does only one thing.

## A worked end-to-end example

The following shows the full identity loop without naming any
specific command. Substitute your toolchain's CLI for each step.

1. **Author.** Create a manifest, write the contract, run a build.
   The lockfile now pins the source hash and the build hashes.

2. **Publish.** Run `publish`. The toolchain writes a version
   metadata entry that names the source hash and the build hashes.
   Commit and push.

3. **Consumer pulls.** A second developer adds the package as a
   dependency and resolves. The resolver fetches the source through
   Git (or the discovery index) and verifies that the recorded
   source hash matches the actual source tree. The resolution writes
   a fresh lockfile for the consumer.

4. **Consumer builds.** The consumer's build computes the six
   build hashes from the resolved source and writes them to the
   consumer's lockfile. If any hash disagrees with what the consumer
   expected, the build fails.

5. **Deploy.** The deploy adapter submits the artifact to a
   network and writes a deployment manifest. The next build run
   copies the deployment references into the consumer's lockfile.

6. **Verify.** The verifier reads the lockfile and the deployment
   manifest and reports a pass or a list of disagreements. With
   `--require-live-evidence`, it also asks the chain whether the
   recorded cell still matches.

If step 4 fails, the consumer knows immediately that something is
wrong. If step 6 fails, the verifier knows whether the problem is in
the source, the build, the deployment, or the chain — and reports
which layer disagrees.

## Recap

Phase 1 gives you:

- content-addressed source identity, with no central server;
- per-build identity that survives toolchain upgrades being treated
  as a deliberate action;
- per-network deployment identity that can be checked locally or
  against a live chain;
- fail-closed verification at every layer, with structured
  disagreements rather than silent overrides.

In exchange, you give up:

- mutable channels like `latest` and `stable`;
- a canonical index that resolves a namespace globally;
- automatic cross-profile reuse;
- the convenience of "yank" and re-publish.

For long-lived, auditable, multi-team contracts, those trade-offs
are usually worth it. For toy experiments, the lack of a `latest`
tag might be annoying, and you should know that before you adopt
the system.