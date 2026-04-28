# iCKB CellScript Expressibility Matrix

Support levels:

- `NATIVE`: direct, clear CellScript surface.
- `EXPRESSIBLE_WITH_PATTERN`: possible with recurring idiom or model helper.
- `EXPRESSIBLE_WITH_ESCAPE_HATCH`: needs raw CKB/Rust/codegen-level logic.
- `NOT_EXPRESSIBLE`: not currently representable.
- `OUT_OF_SCOPE`: off-chain or external protocol behaviour.

| iCKB semantic item | Support | Desired CellScript surface | Current workaround | Compiler/codegen need | Test strategy | Severity |
|---|---|---|---|---|---|---|
| Linear receipt consumption | NATIVE | `receipt R`, `consume receipt` | Used in `ickb_logic.cell` | None | Compile plus model duplicate receipt fixture | LOW |
| DAO deposit/receipt output grouping | EXPRESSIBLE_WITH_PATTERN | `assert_grouped_outputs(DaoDeposit.amount == Receipt.amount * qty)` | Rust model fixture; CellScript resource/receipt fields | Executable aggregate invariant lowering over output groups | Positive deposit phase 1, forged receipt negative | HIGH |
| Deposit min/max unoccupied capacity | NATIVE | `assert_invariant(amount >= min && amount <= max)` | Used in CellScript and fixture model | Better occupied/unoccupied capacity primitive | Capacity violation negative | MEDIUM |
| Header dep accumulated-rate load | NOT_EXPRESSIBLE | `let ar = header_dep(input).dao.accumulated_rate` | Explicit `deposit_accumulated_rate` field plus model flag | HeaderDep source API with fail-closed missing-header behaviour | Missing/wrong header negatives are model-level | BLOCKER |
| Exact iCKB accounting across tokens, receipts, deposits | EXPRESSIBLE_WITH_ESCAPE_HATCH | `assert_sum(inputs<IckbToken>.amount + receipt_value(...)) == ...` | Fixture model checks exact equation | Runtime aggregate lowering, computed aggregate expressions, group scans | Inflation/deflation negatives | BLOCKER |
| Proposal `>=` vs Rust exact equality distinction | EXPRESSIBLE_WITH_PATTERN | Relation operator in aggregate invariant | Model uses Rust exact equality | Diagnostic/reporting support for source/proposal discrepancies | Deflation negative | HIGH |
| xUDT args `[logic_hash, flags]` binding | EXPRESSIBLE_WITH_ESCAPE_HATCH | `type xudt<owner_mode=input_type>(logic_hash)` | `xudt_args_hash: Hash` placeholder | First-class Script and xUDT stdlib ABI constructors | Wrong xUDT binding negative | HIGH |
| Script used as both lock and type in coordinated cells | NOT_EXPRESSIBLE | `current_script_role()`, `cell.lock == self`, `cell.type == self` | Comments and model fixture | Script-role introspection and Source group APIs | Script role confusion negative | BLOCKER |
| Empty args for all current/output script uses | EXPRESSIBLE_WITH_ESCAPE_HATCH | `require_empty_args(self, outputs = true)` | Not represented in CellScript; model fixture | Script args source and output lock/type scan | Witness/script misuse fixture | HIGH |
| CKB occupied/unoccupied capacity | EXPRESSIBLE_WITH_PATTERN | `cell.capacity.unoccupied()` | Explicit field on model resources | Native occupied capacity syscall wrapper and capacity evidence | Capacity fixture | HIGH |
| NervosDAO deposit/withdrawal data classification | EXPRESSIBLE_WITH_ESCAPE_HATCH | `is_dao_deposit(cell)`, `is_dao_withdrawal(cell)` | Model fields and docs | DAO stdlib primitives over type hash and data bytes | Deposit/withdrawal fixtures | HIGH |
| DAO maturity/second withdrawal | EXPRESSIBLE_WITH_ESCAPE_HATCH | `require_dao_maturity(request)` | Explicit epoch fields in model | Since/DAO maturity helper that binds deposit/request headers | Immature redeem negative | HIGH |
| CKB CellDep/deployment dep-group binding | EXPRESSIBLE_WITH_PATTERN | `requires_cell_dep("ickb_dep_group")` | Metadata assumptions and fixture model | Stronger deploy manifest/runtime dep validation | Cell dep substitution negative | HIGH |
| Limit Order C256 checked arithmetic | EXPRESSIBLE_WITH_ESCAPE_HATCH | `checked_u256` or `ckb::u256` | u128 CellScript model | Checked 256-bit arithmetic library/codegen | Underpayment positive/negative | HIGH |
| Limit Order MetaPoint/OutPoint binding | NOT_EXPRESSIBLE | `input.out_point`, `output.index`, signed relative distance | Unsigned model fields | OutPoint/index Source APIs and signed ints | Limit order and Owned-Owner fixtures | BLOCKER |
| Limit Order ratio validation | EXPRESSIBLE_WITH_PATTERN | `Ratio?`, `Option<Ratio>` | Zero-pair sentinel fields | Option/union ergonomic support and Molecule union encoding | Malformed ratio model support | MEDIUM |
| Limit Order partial fill min | NATIVE for arithmetic | `assert_invariant(delta >= min)` | Implemented in `limit_order.cell` | u256 support for production parity | Limit positive/underpayment negative | MEDIUM |
| Owned-Owner signed i32 distance | NOT_EXPRESSIBLE | `i32`, `owner.index + distance == owned.index` | Unsigned indexes | Signed integer types and source index APIs | Owned-Owner positive/wrong-owner negative | HIGH |
| Witness parsing and malformation rejection | EXPRESSIBLE_WITH_PATTERN | `witness T`, explicit ABI | Existing witness params for simple typed data | Full Molecule witness parser and signature primitive | Witness malformation negative is model-level | HIGH |
| CoBuild/OTX UI heuristics | OUT_OF_SCOPE | N/A | Document only | Off-chain tooling | Not tested | LOW |
| Non-upgradable deployment proof | OUT_OF_SCOPE for compiler semantics | Deployment manifest proof | Docs only | Deployment verification tooling | Not tested | MEDIUM |

## Compiler Features Missing For iCKB-Grade Claims

1. Header dep access that binds an input/receipt/deposit to its block header and
   fails closed when absent.
2. First-class Script values, current script hash, current role, lock/type group
   views, and script args inspection.
3. Executable transaction aggregate invariants with computed expressions, not
   only metadata-only proof-plan records.
4. CKB source views for outpoints, output indexes, occupied capacity, and
   outputs/outputs-data alignment as ordinary typed expressions.
5. DAO and xUDT stdlib primitives that match deployed script ABIs.
6. Signed integer types and checked 256-bit arithmetic.
7. Stable diagnostics/error codes for each lowered invariant family.
