# CellScript Compiler Error Codes

Compiler diagnostics use stable numeric ranges. Runtime verifier exits remain
in the low-numbered `E####` registry documented by
`CELLSCRIPT_RUNTIME_ERROR_CODES.md`; backend/compiler failures use the
non-overlapping `E2xxx` range.

| Code | Name | Meaning | Suggested action |
| --- | --- | --- | --- |
| `E2000` | `backend-uncategorized` | Backend failure outside a classified boundary. | Preserve the full diagnostic and the source that reached code generation. |
| `E2001` | `backend-empty-artifact` | Code generation produced no artifact bytes. | Check entrypoint selection and artifact target. |
| `E2100` | `type-layout-lowering` | A type could not be lowered to its backend storage layout. | Check fixed widths, schema fields, and payload layouts. |
| `E2101` | `entry-abi-lowering` | The selected entrypoint could not be lowered to the entry ABI. | Check entry parameters and witness ABI support. |
| `E2102` | `action-lowering` | An action body could not be lowered. | Inspect the named action and operation in the diagnostic. |
| `E2103` | `lock-lowering` | A lock body could not be lowered. | Inspect the named lock and target runtime requirements. |
| `E2104` | `pure-function-lowering` | A pure function could not be lowered. | Check call ABI, return layout, and the reported expression. |
| `E2200` | `unresolved-assembly-symbol` | Generated assembly references a missing label or call target. | Check callable reachability and helper closure. |
| `E2201` | `assembly-layout` | Generated assembly could not form a valid machine layout. | Check sections, labels, branches, and block ordering. |
| `E2202` | `instruction-encoding` | A RISC-V instruction or immediate could not be encoded. | Check the mnemonic, operands, registers, and immediate range. |
| `E2300` | `elf-emission` | A valid RISC-V ELF artifact could not be constructed. | Check entrypoint, section layout, offsets, and size constraints. |
| `E2400` | `external-toolchain` | An explicitly configured RISC-V toolchain failed. | Check the toolchain environment variables, executable paths, and stderr. |
| `E2900` | `backend-invariant` | An internal backend invariant failed after semantic checking. | Retain the source and compiler version and report a compiler defect. |

The CLI exposes these codes in human diagnostics and in the `diagnostics[].code`
field of `--json` failures. The language server publishes them through the LSP
diagnostic `code` field and supplies a `codeDescription` link to this registry.
