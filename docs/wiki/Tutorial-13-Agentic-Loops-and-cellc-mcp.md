# Tutorial 13: Agentic Loops and cellc-mcp

CellScript is a small, narrow language with a deterministic compiler and
machine-readable diagnostics. Those properties make it a good fit for an
agentic loop: a programme writes `.cell` source, asks the compiler what is
wrong, and corrects itself from structured feedback rather than from guessed
prose.

This chapter describes that loop. It explains why the `cellc` surface is shaped
for automated callers, which commands give stable machine-readable output, and
where a reference wrapper (`cellc-mcp`) exposes those commands as agent tools.
It keeps the same boundary as the rest of the wiki: a loop that ends at "the
compiler accepted this" has produced compiler evidence, not CKB chain
acceptance.

## What You Will Learn

- why `cellc` suits a write -> check -> explain -> fix loop;
- which commands emit stable, machine-readable output an agent can act on;
- how stable diagnostic codes and `cellc explain` close the correction loop;
- what a reference MCP wrapper (`cellc-mcp`) exposes, and how an agent uses it;
- the read-vs-write rule that keeps an automated loop safe;
- where the loop's evidence stops, and what still needs builder and CKB
  evidence.

## Why The Compiler Fits A Loop

Hand-written CKB scripts are hard for an automated caller to get right: it must
track inputs, CellDeps, and outputs by index, encode typed state into byte
arrays, and preserve linear-asset semantics by convention. Those are exactly
the details an automated writer gets subtly wrong, and the failure often only
appears at transaction time.

CellScript moves those invariants into the language, so the feedback an
automated caller needs arrives at compile time instead. The compiler is also:

- **narrow**: a small set of keywords and a fixed set of Cell effects such as
  `consume`, `create`, `destroy`, and `transition`, so the space of valid output
  is small enough to hold in context;
- **deterministic**: the same source produces the same diagnostics, so a loop
  can rely on the answer instead of sampling it;
- **machine-readable**: diagnostics carry stable codes and JSON shape, so a
  caller acts on a code rather than parsing English.

The result is a tight loop: the writer proposes source, the compiler returns a
structured verdict, and the writer corrects from it. The compiler is the oracle;
the writer never has to guess whether its contract is well formed.

## The Machine-Readable Surface

The loop is built from commands that already emit JSON. Run them from a package
directory that contains `Cell.toml`; the `.` argument refers to the current
package when a command expects an explicit path.

`cellc check` is the core of the loop. It type-checks and lowers the package and
emits a JSON summary:

```bash
cellc check --target-profile ckb --json
```

On success the summary reports `"status": "ok"`. On failure it reports
`"status": "failed"` together with counts and a `diagnostics` array. Each
diagnostic carries the fields an automated caller needs to locate and classify
the problem:

```json
{
  "status": "failed",
  "error_count": 1,
  "warning_count": 0,
  "diagnostics": [
    {
      "message": "a proposed output failed its declared field transition check",
      "severity": "error",
      "code": "E0014",
      "span": { "line": 21, "column": 9, "start": 360, "end": 372 }
    }
  ]
}
```

The `code` is the important field. Diagnostic codes are the stable correction
signal: a caller can branch on `E0014` rather than parse the rendered message.

Two more read commands give the writer context without leaving the loop:

```bash
cellc explain E0014
cellc constraints . --target-profile ckb --json
```

`cellc explain` turns a code into a description and a fix hint. `cellc
constraints` lets the writer inspect what the compiler believes the contract
reads, writes, creates, consumes, and is obliged to verify.

`cellc metadata` is also useful, but it is commonly run with an output path:

```bash
cellc metadata . --target riscv64-elf --target-profile ckb -o /tmp/metadata.json
```

Treat that as an explicitly confirmed inspection step rather than as part of an
unattended read-only loop, because it writes a file even though it still
produces compiler evidence.

## The Loop

With those commands, the loop is small:

1. The writer proposes `.cell` source for the package.
2. Run `cellc check --target-profile ckb --json`.
3. If `status` is `ok`, stop.
4. Otherwise, for each diagnostic, optionally run `cellc explain <code>`,
   revise the source, and return to step 2.

A bounded iteration count keeps a non-terminating writer from looping forever.
In practice a capable writer converges in a few rounds: the first check finds a
syntax or transition mistake, the explain output names the fix, and the next
check passes.

The value of this loop over an unchecked writer is that every claim of success
is backed by the compiler. A writer that says "this token contract is valid" has
either passed `cellc check` or it has not, and the loop knows which.

## A Worked Correction

Suppose a writer proposes a mint action whose output does not satisfy its
declared transition. `cellc check --json` returns `status: failed` with code
`E0014` (`mutate-transition-mismatch`). The writer calls `cellc explain E0014`,
learns that a proposed output failed its declared field transition check, and
revises the action so the `transition` and the constructed output agree. The
next `cellc check` returns `status: ok`. No human needs to read the intermediate
error; the stable code and the explain text carried the correction.

The bundled examples are a good source of grounding for a writer that has not
seen CellScript before. Pointing the writer at `examples/token.cell` and the
other bundled contracts gives it correct patterns to imitate, which shortens the
first few rounds of the loop. See
[Bundled Example Contracts](Tutorial-08-Bundled-Example-Contracts.md).

## The In-Repo MCP Server: cellscript-mcp

A loop needs the compiler available as callable tools. This repository now
ships the `cellscript-mcp` binary, a small Model Context Protocol server that
exposes read-oriented compiler commands and project documentation as agent
tools, so an MCP-capable model can call them directly.

The server is deliberately thin. It owns only the boundary work an agent caller
needs: locating `cellc`, running read-only commands, preserving stdout/stderr,
and returning structured evidence. Compilation still belongs to `cellc`.
Because the server invokes the same `cellc` binary, the diagnostics a model
sees are the compiler's, not a re-implementation.

The default tool set is read-only:

- `cellscript_command_tree`
- `cellscript_check`
- `cellscript_constraints`
- `cellscript_metadata`
- `cellscript_template_layouts`
- `cellscript_protocol_graph`
- `cellscript_explain`
- `cellscript_gate_policy`
- `cellscript_docs_topic`
- `cellscript_evidence_levels`

The server does not expose signing, publish, deployment submission, registry
mutation, or editor/shell configuration mutation by default.

Any MCP client can drive it; the wrapper does not assume a particular model. A
local model that has little or no CellScript in its training data benefits most,
because the narrow language surface fits in context and the deterministic
compiler supplies the correctness signal the model lacks on its own.

## Read Automatically, Write On Confirmation

An automated loop should run read-only commands freely and gate anything that
writes. The core commands in this chapter, `check`, `explain`, and
`constraints`, only read source and print reports, so a loop can call them
without supervision. That is what makes the write -> check -> fix cycle safe to
automate.

Anything that produces an artifact, writes a report, or touches state is a
different class. Writing a checked contract to disk, producing metadata with
`-o`, building an ELF, or preparing a transaction should pass through an
explicit confirmation step rather than run inside the automatic loop. Keeping
that line, read freely and confirm before writing, lets the compiler-in-the-loop
stay fast while a human keeps the gate on side effects.

## Where The Loop's Evidence Stops

This is the same boundary the rest of the wiki keeps. A loop that ends at
`cellc check` passing has produced **compiler evidence**: the source is well
formed, the effects and transitions are consistent, and the target profile
accepted it. It has not produced **CKB chain evidence**.

A passing check does not prove that a builder can spend the right input Cells,
serialise the right witness, satisfy capacity, pass dry-run, and commit. As
[Metadata, Verification, and Production Gates](Tutorial-06-Metadata-Verification-and-Production-Gates.md)
explains, that distinction is what prevents overclaiming. An agentic loop makes
the compiler-evidence half fast and autonomous; it does not move the chain-
evidence half. A contract a model wrote and checked is a draft for review, not a
deployment.

For the chain-facing half, the loop hands off to the same release-facing
evidence the rest of the wiki describes: builder generation, builder tests, and
the CKB acceptance gate, none of which an automatic loop should run unattended:

```bash
cellc build --target riscv64-elf --target-profile ckb --json
cellc verify-artifact build/main.elf --expect-target-profile ckb
./scripts/cellscript_gate.sh release
```

## Next

You have now seen the full local picture: the language, the package and profile
workflow, metadata and production gates, editor tooling, bundled examples, the
registry flow, and the agentic loop that ties the read-oriented compiler surface
together. For the chain-facing evidence an agent loop deliberately stops short
of, return to
[Metadata, Verification, and Production Gates](Tutorial-06-Metadata-Verification-and-Production-Gates.md).
