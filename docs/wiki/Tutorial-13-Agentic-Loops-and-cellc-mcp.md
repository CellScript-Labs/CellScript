CellScript is a small, narrow language with a deterministic compiler and
machine-readable diagnostics. Those three properties make it a good fit for an
agentic loop: a program — often a language model — writes `.cell` source, asks
the compiler what is wrong, and corrects itself from the structured answer,
without a human reading every intermediate error.

This chapter describes that loop. It explains why the `cellc` surface is already
shaped for automated callers, which commands give stable machine-readable
output, and where a reference wrapper (`cellc-mcp`) exposes those commands as
agent tools. It keeps the same boundary the rest of the wiki keeps: a loop that
ends at "the compiler accepted this" has produced compiler evidence, not chain
acceptance.

## What You Will Learn

- why `cellc` suits a write → check → explain → fix loop;
- which commands emit stable, machine-readable output an agent can act on;
- how stable error codes and `cellc explain` close the correction loop;
- what a reference MCP wrapper (`cellc-mcp`) exposes, and how an agent uses it;
- the read-vs-write rule that keeps an autonomous loop safe;
- where the loop's evidence stops, and what still needs builder and CKB evidence.

## Why the Compiler Fits a Loop

Hand-written CKB scripts are hard for an automated caller to get right: it must
track inputs, CellDeps, and outputs by index, encode typed state into byte
arrays, and preserve linear-asset semantics by convention. Those are exactly the
details an automated writer gets subtly wrong, and the failure usually only
appears at transaction time.

CellScript moves those invariants into the language, so the feedback an
automated caller needs arrives at compile time instead. The compiler is also:

- **narrow** — a small set of keywords and a fixed set of Cell effects
  (`consume`, `create`, `destroy`, `transition`), so the space of valid output
  is small enough to hold in a prompt;
- **deterministic** — the same source produces the same diagnostics, so a loop
  can rely on the answer instead of sampling it;
- **machine-readable** — diagnostics carry stable codes and JSON shape, so a
  caller acts on a code, not on prose.

The result is a tight loop: the writer proposes source, the compiler returns a
structured verdict, and the writer corrects from it. The compiler is the oracle;
the writer never has to guess whether its contract is well formed.

## The Machine-Readable Surface

The loop is built from commands that already emit JSON. Run them from a package
directory that contains `Cell.toml`; the `.` argument refers to the current
package.

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

The `code` is the important field. CellScript runtime error codes are stable and
documented, and the same registry is exposed in metadata schema constraints, so
`cellc check --json`, the metadata sidecar, and the explain command all agree on
the same identifiers. A caller can branch on `E0014` rather than parse English.

Two more commands give the writer context without leaving the loop:

```bash
cellc explain E0014
cellc metadata . --target riscv64-elf --target-profile ckb -o /tmp/metadata.json
cellc constraints . --target-profile ckb
```

`cellc explain` turns a code into a description and a fix hint. `cellc metadata`
and `cellc constraints` let the writer inspect what the compiler believes the
contract reads, writes, creates, consumes, and is obliged to verify — useful
when the writer needs to reason about semantics rather than syntax.

## The Loop

With those commands, the loop is small:

1. the writer proposes `.cell` source for the package;
2. run `cellc check --target-profile ckb --json`;
3. if `status` is `ok`, stop;
4. otherwise, for each diagnostic, optionally `cellc explain <code>`, revise the
   source, and return to step 2.

A bounded iteration count keeps a non-terminating writer from looping forever. In
practice a capable writer converges in a few rounds: the first check finds a
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
next `cellc check` returns `status: ok`. No human read the intermediate error;
the stable code and the explain text carried the correction.

The bundled examples are a good source of grounding for a writer that has not
seen CellScript before. Pointing the writer at `examples/token.cell` and the
other bundled contracts gives it correct patterns to imitate, which shortens the
first few rounds of the loop. See
[Bundled Example Contracts](https://github.com/CellScript-Labs/CellScript/wiki/Tutorial-08-Bundled-Example-Contracts).

## A Reference Wrapper: cellc-mcp

A loop needs the compiler available as callable tools. `cellc-mcp` is a
reference wrapper — a small Model Context Protocol server — that exposes the
read-only commands above as agent tools, so an MCP-capable model can call them
directly. It is published at
[github.com/toastmanAu/cellc-mcp](https://github.com/toastmanAu/cellc-mcp).

The wrapper is deliberately thin. It owns only the boundary work an agent caller
needs — locating the binary, running each command, and presenting the JSON in a
form sized for a model's context — and leaves all compilation to `cellc`. The
tools it exposes are read-only: type-check a contract, explain a code, read
metadata, list and fetch bundled examples, and return the language reference.
Because the wrapper invokes the same `cellc` binary, the diagnostics a model
sees are the compiler's, not a re-implementation.

Any MCP client can drive it; the wrapper does not assume a particular model. A
local model that has little or no CellScript in its training data benefits most,
because the narrow language surface fits in context and the deterministic
compiler supplies the correctness signal the model lacks on its own.

## Read Auto, Write on Confirmation

An autonomous loop should run read-only commands freely and gate anything that
writes. The commands in this chapter — `check`, `explain`, `metadata`,
`constraints` — only read; they produce no files and touch no chain, so a loop
can call them without supervision. That is what makes the write → check → fix
cycle safe to automate.

Anything that produces an artifact or touches state is a different class. Writing
a checked contract to disk, building an ELF, or preparing a transaction should
pass through an explicit confirmation step rather than run inside the automatic
loop. Keeping that line — read freely, confirm before writing — lets the
compiler-in-the-loop stay fast while a human keeps the gate on side effects.

## Where the Loop's Evidence Stops

This is the same boundary the rest of the wiki keeps. A loop that ends at
`cellc check` passing has produced **compiler evidence**: the source is well
formed, the effects and transitions are consistent, and the target profile
accepted it. It has not produced **CKB chain evidence**.

A passing check does not prove that a builder can spend the right input Cells,
serialize the right witness, satisfy capacity, pass dry-run, and commit. As
[Metadata, Verification, and Production Gates](Tutorial-06-Metadata-Verification-and-Production-Gates.md)
explains, that distinction is what prevents overclaiming. An agentic loop makes
the compiler-evidence half fast and autonomous; it does not move the chain-
evidence half. A contract a model wrote and checked is a draft for review, not a
deployment.

For the chain-facing half, the loop hands off to the same release-facing
evidence the rest of the wiki describes — builder generation, builder tests, and
the CKB acceptance gate — none of which an automatic loop should run unattended:

```bash
cellc build --target riscv64-elf --target-profile ckb --json
cellc verify-artifact build/main.elf --expect-target-profile ckb
./scripts/cellscript_gate.sh release
```

## Next

You have now seen the full local picture: the language, the package and profile
workflow, metadata and production gates, editor tooling, the bundled examples,
and the agentic loop that ties the read-only compiler surface together. For the
chain-facing evidence an agent loop deliberately stops short of, return to
[Metadata, Verification, and Production Gates](https://github.com/CellScript-Labs/CellScript/wiki/Tutorial-06-Metadata-Verification-and-Production-Gates).
