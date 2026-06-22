# CellScript Molecule / IFRN 设计空间改进闭环报告

日期：2026-06-22
分支：`nightly-0.20`
范围：当前 CellScript 仓库、NovaSeal / DOB / iCKB 相关样例与测试资产，以及本轮 Infern / IFRN 讨论中暴露的问题。
说明：本报告由原设计空间审计报告改写为改进闭环报告。它不重新审计外部 Infern 仓库源码；关于 Infern 六个合约的结论仍基于用户提供的模式描述，并以当前 CellScript 能力边界作技术对账。

## 闭环状态

截至 2026-06-22，本报告中属于 CellScript 仓库内可修复的 P0/P1 设计空间压缩项已完成闭环：raw cell-data codec honesty、CKB deployable artifact identity、DOB devnet/registry pressure、NovaSeal Agreement BIP340 verifier IPC、variable-length packed hash、wide/nested fixed packed struct hashing，以及 iCKB committed evidence matrix 均已通过对应 gate。

本轮没有修改 Agreement 语义；修正集中在 compiler/backend、runtime diagnostic、packed preimage materialisation、BIP340 verifier syscall/IPC wiring、planned-profile fixture hash 语义对齐和 evidence refresh。NovaSeal / DOB / bundled examples 的本地 source-package production 层已闭环；剩余条目不再作为当前 blocker，而是 public/mainnet release 或产品化路线项：external codec adapter、raw-layout DSL、多 ABI builder codec backend、registry/indexer 多 ABI 支持、声明式 continuity/timepoint policy，以及 Infern 六合约真实 port parity matrix。

### 已通过门禁

| Gate | 状态 | 说明 |
|---|---|---|
| NovaSeal Agreement live devnet stateful | passed | valid originate/repay/claim 路径通过；签名负例以 BIP340 child verifier rejection 语义失败，不再落入 code 1 / code 18。 |
| NovaSeal core live devnet stateful | passed | wrong-signature transition 以 verifier rejection 失败。 |
| NovaSeal planned profiles live devnet stateful | passed | btc-transaction-commitment、btc-utxo-seal、dual-seal、fiber-candidate、fungible-xudt、rwa-receipt 六个 profile 的 valid path 和语义负例均通过 live devnet runner。 |
| DOB evolving devnet workflow | passed | proposal-local workflow 脚本通过。 |
| DOB registry pressure | passed | registry pressure gate 通过。 |
| packed hash regressions | passed | wide fixed-width、multi-block、nested fixed-width、agreement-sized nested parameter、signature payload field hashing 均通过 CKB VM hash 对比；有效路径不再可达 code 18。 |
| iCKB differential matrix | passed | refreshed committed dual-side CKB VM evidence 后，218 个 `ickb_diff` tests 正常通过。 |
| iCKB benchmark | passed | 5 个 benchmark/fixture model tests 通过。 |
| iCKB claim manifest verifier | passed | `cellc verify-ckb-fixtures` 等价命令通过，manifest status 为 `complete-executable-claim-set`。 |
| Rust format/check | passed | `cargo fmt --all --check` 与 `cargo check --locked -p cellscript --all-targets` 通过。 |

## 执行摘要

### 结论分级

| 类别 | 结论 | 证据层级 |
|---|---|---|
| 已验证事实 | `nightly-0.20` 的 CKB profile、runtime ABI validation、schema manifest、schema-backed dynamic witness payload 和 public scheduler witness 仍是 Molecule-first / Molecule-only。 | repo-proven |
| 已验证事实 | `ckb::cell_data_*`、OutPoint helpers、`lock_args` 等 escape hatch 存在；NovaSeal `v0-mvp-skeleton` 已经用 byte-offset raw layout 和 OutPoint 绑定表达非 Molecule-style cell data 检查。 | repo-proven |
| 已验证事实 | iCKB executable specs 实际位于 `tests/benchmarks/ickb_specs/`，并且该目录的 README 明确说明它们故意不放进 public examples tree；旧 roadmap 中的 `examples/ickb_benchmark/*.cell` 表述是 stale claim。 | repo-proven |
| 已验证事实 | CellScript 0.20 已把 CKB ELF loader / ABI boundary 纳入 acceptance evidence；本轮修正后，production acceptance report 也新增 `cellscript_build_reports`，把 compiled ELF hash、`verify-artifact`、ELF entry ABI 和 live code-cell data hash 绑定到同一 artifact。 | repo-proven |
| 已验证事实 | 本轮修正后，compile metadata 新增 `cell_data_codec_manifest`；Molecule-native 合约声明 `abi = "molecule"`，使用 raw `LOAD_CELL_DATA` 的合约声明 `abi = "molecule+raw-bytes-v1"`，并且 `cellc gen-builder --target typescript` 会把该 manifest 导出到 builder manifest 和 action plan。 | repo-proven |
| 工程推断 | 如果 Infern 六合约确实采用 IFRN/raw byte layout，则当前 CellScript source-level 很可能可表达；但 TypeScript builder、codec、registry/indexer tooling 不能直接复用 Molecule metadata。 | engineering-inference |
| 产品判断 | CellScript 当然是 compiler；但当前最强证据在 Molecule-native typed-cell、metadata、audit、provenance、builder identity 这条线。raw/IFRN production compiler claim 还需要 parity gates。 | product-judgement |

### 证据锚点

| Claim | 当前锚点 |
|---|---|
| CKB profile / metadata Molecule hard constraint | `src/lib.rs:306`, `src/lib.rs:874`, `src/lib.rs:2739`, `src/lib.rs:2817` |
| public scheduler policy witness Molecule-only | `src/lib.rs:3253-3275` |
| `lock_args` 是 fixed-width `Script.args` escape hatch | `docs/CELLSCRIPT_ENTRY_WITNESS_ABI.md:40-43` |
| schema-backed dynamic witness payload 是 Molecule data | `docs/CELLSCRIPT_ENTRY_WITNESS_ABI.md:70-80` |
| raw cell data / OutPoint helpers 存在 | `docs/CELLSCRIPT_0_18_ROADMAP.md:93-110` |
| NovaSeal v0-mvp raw byte-offset 反例 | `proposals/novaseal/v0-mvp-skeleton/src/nova_state_lifecycle_type.cell:138-177`, `proposals/novaseal/v0-mvp-skeleton/src/nova_state_type.cell:125-155` |
| v0-mvp packed layout 仍非 production ABI | `proposals/novaseal/v0-mvp-skeleton/docs/SCHEMA_LAYOUT.md:44-54` |
| 新 NovaSeal profiles 多数走 whole-cell packed hash | `proposals/novaseal/fungible-xudt-profile-v0/src/nova_fungible_xudt_lifecycle_type.cell:226-227`, `proposals/novaseal/btc-transaction-commitment-profile-v0/src/nova_btc_transaction_commitment_type.cell:361`, `proposals/novaseal/fiber-candidate-profile-v0/src/nova_fiber_candidate_type.cell:378` |
| iCKB specs 位置与旧 roadmap public examples claim 不一致 | `tests/benchmarks/ickb_specs/README.md:3-9`, `tests/benchmarks/ickb_diff/claim_manifest.json:5-9`, `roadmap/CELLSCRIPT_ROADMAP.md`, `roadmap/CELLSCRIPT_ROADMAP_OVERVIEW.md` |
| 0.20 已有 ELF entry ABI gate，且本轮新增 build report linkage | `docs/releases/CELLSCRIPT_0_20_RELEASE_NOTES.md`, `scripts/ckb_cellscript_acceptance.sh`, `scripts/validate_ckb_cellscript_production_evidence.py`, `docs/CELLSCRIPT_GATE_POLICY.md` |
| `cell_data_codec_manifest` 与 generated builder 暴露 | `src/lib.rs`, `src/cli/commands.rs`, `tests/cli.rs`, `docs/releases/CELLSCRIPT_0_20_RELEASE_NOTES.md` |
| DOB-EVO 主要是 lock-hash / production policy 问题，不是 Molecule-only 问题 | `docs/0.20/CELLSCRIPT_0_20_DOB_EVO_SWARM_AUDIT.md:64`, `docs/0.20/CELLSCRIPT_0_20_DOB_EVO_SWARM_AUDIT.md:121-136` |

### 需要决策的问题

| 选项 | 适用场景 | 工程量 | 主要风险 | 可宣称能力 |
|---|---|---:|---|---|
| Molecule-first + raw escape hatch 文档化 | 短期稳态，避免扩大表面积 | 低 | IFRN / raw builder 仍需手写 | Molecule-native first-class，raw source pattern 可用 |
| External codec adapter | Infern spike 或少量自定义 ABI 集成 | 中 | metadata / adapter contract 必须 byte-for-byte gate | 可集成 custom codec，但 compiler core 不承诺 IFRN |
| First-class raw ABI metadata | 多 ABI 产品化 | 高 | 牵动 builder、registry、indexer、witness、test vectors | raw-layout first-class |
| Full production compiler parity promise | 对标 hand-written Rust 合约迁移 | 很高 | 需要真实项目 cycle / size / behaviour 矩阵 | production compiler claim |

### 推荐路线

1. Phase 0：truth gates。已落地最小 `CellScriptBuildReport`：同一个 CKB-deployable artifact 贯穿 compiled ELF hash、verify-artifact、ELF entry ABI、live code cell data hash。下一步再把 Cell.lock、Deployed.toml、`cell_data_codec_manifest`、valid/tampered carrier 全部并入同一 identity chain。
2. Phase 1：IFRN/raw spike。逐个验证六个合约的 source expressibility、hand-written codec、valid/invalid result、cycles、binary size、tx size、occupied capacity。
3. Phase 2：metadata honesty。最小 `cell_data_codec_manifest` 已落地；下一步补 external codec adapter identity、roundtrip vectors、codec adapter hash、Cell.lock / Deployed.toml linkage。不要让 raw layout 再伪装成 Molecule-only schema。
4. Phase 3：first-class multi-ABI tooling。将 builder、registry、indexer、entry witness payload、scheduler witness 全链路接入多 codec gate。

## 一、总判断

CellScript 的问题不是「完全不能表达非 Molecule 合约」，而是更精确的三层分裂：

1. 链上源层面：设计空间没有被 Molecule-only 大面积锁死。`ckb::cell_data_*`、`ckb::input_out_point_*`、`ckb::require_input_out_point`、`ckb::hash_data_packed` 和 `lock_args` 形成了实际 escape hatch。NovaSeal `v0-mvp-skeleton` 已经证明 hand-rolled byte layout 可以跑在 CellScript-emitted RISC-V 上。
2. 元数据与工具链层面：Molecule 仍是 first-class 硬约束。schema manifest、schema-backed dynamic witness payload、public scheduler policy witness 仍是 Molecule-first / Molecule-only；本轮新增的 `cell_data_codec_manifest` 已能诚实声明 raw `LOAD_CELL_DATA` 需求，但 TypeScript builder 仍没有独立 value codec backend。
3. 产品化层面：CellScript 是 compiler，但当前证据最强的是 spec / audit / provenance / builder identity / registry 工具体系；「任意生产 CKB Rust 合约的替代编译器」这个 claim 需要额外 parity gates。这个定位差异需要被写进 roadmap 和验收门禁。

因此，基于当前已描述的 IFRN raw-layout 模式，Infern / IFRN 的主要风险不应先假定为「CellScript 源码不能 port」，而应先验证为「source-level likely expressible，但 off-chain codec / builder / registry tooling 需要 external codec adapter 和 parity gates」。完整 port 仍需逐脚本 spike；如果走 raw-layout CellScript 路线，必须补 hand-written encoder 或 external codec adapter，不能把 raw layout 当成 Molecule encoder 自动生成。

## 二、已识别问题

### 1. 产品定位过宽，成熟度不均衡

CellScript 同时承担六种角色：

- CKB 合约编译器；
- 形式语义规约；
- ProofPlan / audit oracle；
- package / deployment 身份系统；
- TypeScript action builder；
- 链上 registry verifier。

这几个角色的成熟度不一致。当前最强的是 spec、metadata、deployment identity、audit package 和 registry/provenance 这条线；最缺证据的是「任意生产 CKB Rust 合约迁移后仍保持 cycle、size、行为 parity」这一生产编译器命题。

风险：市场会把 CellScript 理解成「可替换 ckb-std Rust 的生产编译器」，但现有证据更支持「强 metadata / audit / provenance 工具链，加上正在 hardened 的 CKB backend」。

### 2. 生产级 parity 尚未被普遍证明

0.17/0.18 的 iCKB 差分证据是针对声明范围内的 benchmark claim set，不是对任意生产合约集的泛化证明。

当前缺口：

- 没有公开的「CellScript-emitted RISC-V vs 手写 ckb-std Rust」跨真实项目的 cycle / size / behaviour 矩阵；
- 没有针对 Infern 六个脚本这种小体量、成本敏感合约的 production parity benchmark；
- iCKB 源级 specs 实际存在于 `tests/benchmarks/ickb_specs/*.cell`；
- `tests/benchmarks/ickb_specs/README.md` 明确说明这些 specs 是 iCKB-inspired benchmark，不是 audited iCKB Rust scripts 的 faithful port，并且故意放在 `tests/benchmarks` 而不是 public examples tree；
- 因此旧 roadmap 中 `examples/ickb_benchmark/*.cell` 的说法是 stale documentation claim，不应通过复制一份 public example 来伪装解决。

风险：没有 cycle / size 数字时，任何「生产 Rust 合约迁到 CellScript」的建议都缺乏成本依据。

### 3. CKB backend evidence 已开始统一，但 identity chain 仍未全覆盖

CellScript 仓库内已经把 CKB-facing acceptance hardening 写成 0.20 事项：release notes 说明 acceptance path 会在把 local devnet 结果当作 release evidence 之前检查 ELF loader / ABI boundary，acceptance script 和 production evidence validator 也都有 `ckb_elf_entry_abi_gate`。

这说明 CKB backend / deploy artifact shape 已经是 CellScript 自身的 release risk 面，而不是外部项目的附带问题。本轮已经把最小 BuildReport 写进 acceptance report、release docs 和 production evidence validator：每个 report row 记录 compiled ELF 的 CKB data hash、SHA-256、`verify-artifact` status、ELF entry ABI status、ABI trailer stripped 状态，并在 devnet run 中要求 live code-cell data hash 等于该 artifact hash。

仍未完全覆盖的部分是更外层的 package / registry identity：Cell.lock、Deployed.toml、`cell_data_codec_manifest`、builder manifest、valid/tampered carrier 还没有全部被同一个 BuildReport row 贯穿。

风险：本地 verifier 或 `ckb-testtool` 通过只能证明一部分 backend 行为，不能替代真实 CKB node smoke。

### 4. Release caveat：Artifact profile 边界不够清晰

当前需要明确区分：

- compiler internal artifact；
- debug / test artifact；
- CKB-deployable bare ELF；
- metadata-bearing typed-cell artifact；
- stripped / unstripped ABI trailer 状态；
- `data` / `data1` / `data2` deploy profile。

这条是 release evidence caveat，不是单一 source-level compiler bug。本轮已经把 exact-artifact BuildReport 和 live CKB code-cell data-hash match 变成 production evidence validator 的硬要求；剩余缺口是把 package / registry / codec identity 也接进同一 report。

风险：如果本地测试、hash report、部署上链使用的不是同一类 artifact，就会出现「A 被测、B 被部署、C 被登记」的生产风险。

### 5. Release caveat：typed-cell metadata 强，但 safety claim 需要 on-chain pairing

typed-cell metadata 可以给 scheduler、conflict key、typed data hash、ProofPlan 提供强上下文，但 metadata 本身不是链上 enforcement。这里应区分两类 metadata：

- informational / provenance metadata：用于审计、发现、构建身份和可追溯性；
- safety-enforcing metadata：声称某个链上约束已经被执行或绑定。

safety-enforcing metadata 必须配上真实 CKB type script 检查，例如：

- output data hash 必须匹配 type args；
- outputs data 必须承载预期 package commitment；
- tampered output data 必须被 live CKB script verification 拒绝。

风险：如果把 provenance metadata 当成 safety evidence，off-chain audit 看起来完整，链上约束却不一定成立。

### 6. stdlib / syscall surface 仍需真实 workload 压力测试

本轮 CellScript / IFRN 对账说明，CellScript 的 CKB primitive surface 不是一次性完整的。`ckb::cell_data_hash`、`cell_data_hash_at`、`cell_data_u64_le`、OutPoint helpers、HeaderDep / Script / group input-output 语义都需要被真实协议 verifier 持续压测。

风险：demo contracts 和 Molecule-native examples 很难覆盖真实协议会用到的 byte-level / source-view / deployment 组合。

### 7. Molecule metadata 是硬约束

当前硬约束集中在这些位置：

- CKB target profile 固定为 `vm_abi = "ckb-molecule"`；
- `metadata.runtime.vm_abi.format != "molecule"` 会 fail-closed；
- `molecule_schema.abi != "molecule"` 会 fail-closed；
- `molecule_schema_manifest.abi != "molecule"` 会 fail-closed；
- public compiled scheduler policy witness bytes 只支持 Molecule；
- schema-backed dynamic witness 参数 payload 是 Molecule data；
- `cellc gen-builder --target typescript` 从 metadata 生成 action-plan package、metadata、runtime contract 和 typed params；它当前不是完整 transaction materializer，也没有独立 value codec backend。

这里不能把所有 ABI 面混成一个开关。至少要拆成四层：

- `runtime.vm_abi`：CKB VM / target profile 层；
- cell data codec / `cell_data_codec_manifest`：cell data 如何 encode/decode；
- entry witness envelope 与 payload codec：`CSARGv1` envelope 可以复用，schema-backed dynamic payload 当前是 Molecule；
- public scheduler policy witness codec：当前 fail-closed 到 Molecule。

本轮已经补上最小 `cell_data_codec_manifest`，使 raw `LOAD_CELL_DATA` 访问不再被 metadata 伪装成纯 Molecule。仍未完成的是 full raw layout DSL、external adapter identity、test vectors、entry witness payload 多 codec，以及 scheduler witness 多 codec。

风险：合约源可以 hand-roll raw bytes；如果 codec manifest、builder manifest、lockfile 和 adapter identity 没有被共同验证，链上逻辑读 raw layout，链下工具仍可能按错误 codec encode/decode。

### 8. Off-chain builder / codec / indexer 设计空间被压缩

Molecule-only 对链上源表达不是大 blocker，但对依赖当前 metadata ABI 的 tooling 是硬约束：

- TypeScript builder；
- entry witness encoder；
- registry / build identity verifier；
- indexer、SDK、explorer 或 adapter decode；
- off-chain audit tooling；
- deployment metadata conformance；
- package / schema hash 绑定。

对 Infern / IFRN 这种自定义二进制布局，当前结果是：

- CellScript source 可以按 byte offset 写；
- `cellc gen-builder` 已能导出 `cell_data_codec_manifest` 并把 raw bytes 需求放入 action plan；
- 当前 `cellc gen-builder` 仍没有 value codec backend，不能自动生成 IFRN encoder；
- 如果 Infern 六合约确为自定义 raw layout，则 `infern-codec` 或等价 TS/Rust codec 必须手写，或作为 external codec adapter 进入 manifest；
- 若 codec manifest、adapter manifest 和 test vectors 不进 gate，任何依赖 schema manifest 的 off-chain 工具仍会被误导。

### 9. Cross-cell / OutPoint 问题的准确表述应改成 not-tested，而不是 missing

早期「cross-cell 验证是洞」的说法过强。当前更准确：

- `lock_group + transaction scope` 是合法 pattern，但会有 risky coverage diagnostic；
- OutPoint helpers 已存在，`ckb::input_out_point_tx_hash` / `input_out_point_index` / `require_input_out_point` 已覆盖一部分场景；
- examples 和多数 NovaSeal profiles 没有系统测试「扫 inputs 找匹配 outpoint」这类模式；
- iCKB specs 中存在 OutPoint / MetaPoint 使用；
- Infern 的具体 `require_input_with_outpoint(field, outpoint)` 模式需要逐条 spike，不能仅靠 iCKB gap matrix 推断。

风险：devgates 通过说明当前测试范围通过，不说明常见 CKB cross-cell pattern 已被充分覆盖。

### 10. 时间 / 连续性不是一等声明式能力

生产 Cell model 常见的时间与连续性约束包括：

- cooldown；
- `last_settled_at`；
- listing `updated_at`；
- monotonic nonce；
- expiry；
- since / header / timepoint；
- state continuity。

这不是说 CellScript 没有时间能力：`env::current_timepoint()` 和手写 generation / expiry / owner continuity 检查已经存在。问题是这些检查还没有形成声明式 continuity policy / timepoint policy。

风险：语言宣称 typed Cell model，但时间维仍停留在手写 verifier 逻辑，审计和 builder 无法从 metadata 中稳定复用这些语义。

### 11. Dev gates 没发现这些问题的原因

现有 gates 通过并不矛盾，原因是它们没有覆盖相同风险面：

- examples 主要覆盖 protected / witness / read / consume / create 等 Molecule-friendly cell-internal pattern；
- NovaSeal 多数 profiles 使用 `hash_data_packed(value) == cell_data_hash(source)`，不是 raw IFRN codec roundtrip；
- v0-mvp-skeleton 覆盖 raw byte-offset 源表达，但没有证明 `gen-builder` 能生成 raw codec；
- `ckb-testtool` 不能替代 live CKB node，且 tamper rejection 需要证明是合约语义拒绝，而不是 capacity、missing CellDep、wrong hash_type 或 malformed tx 这类外围失败；
- 没有非 Molecule ABI metadata fixture；
- 没有 raw-layout builder roundtrip gate；
- 没有 Infern/Rust-to-CellScript cycle / size parity matrix；
- roadmap claim 和实际资产位置没有自动一致性检查。

## 三、范围校正

### DOB-EVO 不是 Molecule-only 证据

DOB-EVO 主要是 typed resource / owner-lock / lifecycle continuity 模式，不是 `require_input_out_point` 的主要证据，也不是 Molecule-only 的主要证据。此前 DOB audit 中更关键的约束是 runtime-loaded `cell_lock_hash` 与 witness-derived hash 的 fixed-byte comparison / production policy 问题。

风险：把 DOB-EVO 归因到 Molecule-only 会误判真实边界。它暴露的是 lock-hash comparison 和 policy-gate 层的问题，应作为 scope adjustment 处理，而不是作为 Molecule ABI finding。

### NovaSeal v0-mvp 是 escape hatch 证据，不是稳定 ABI 结论

NovaSeal `v0-mvp-skeleton` 证明 CellScript source-level 可以表达 raw byte-offset layout、exact size checks 和 OutPoint binding。但它本身的 schema layout 文档也说明 packed reference layout 不是 full production encoding，不生成 Molecule schema，也没有对齐 CellScript compiler ABI metadata。

同时，较新的 NovaSeal profiles 更多使用 `ckb::cell_data_hash(source) == ckb::hash_data_packed(value)` 这种 whole-cell packed hash 模式，而不是继续扩展 byte-offset pattern。这个事实强化了本报告的结论：byte-offset 是可用 escape hatch，但如果要产品化 raw layout，需要正式的 codec metadata、test vectors 和 builder/indexer support。

## 四、被挤压的设计空间

### 没有明显被挤压的空间

1. 链上 source-level verifier 逻辑：可以用 `ckb::cell_data_*` 读取 raw layout。
2. 固定宽度 `Script.args`：`lock_args` 提供非 Molecule witness 的窄通道。
3. Typed struct hash commitment：`ckb::hash_data_packed(value)` 可用于 CellScript packed typed value commitment；它不是通用 raw-bytes / IFRN encoder 的替代品。
4. CKB OutPoint 读取：`input_out_point_tx_hash` / `input_out_point_index` / `require_input_out_point` 已覆盖基础场景。
5. 手写 layout 合约：NovaSeal `v0-mvp-skeleton` 证明可行。

### 被明显挤压的空间

1. 非 Molecule codec metadata：最小 `cell_data_codec_manifest` 已能诚实表达 `molecule+raw-bytes-v1`，但 external adapter identity、adapter hash 和 test vectors 还没有成为同一条 production evidence chain。
2. 自动 TypeScript builder：metadata-driven builder 目前没有 value codec backend，无法自动生成 IFRN encoder。
3. Registry / indexer / SDK decode：链下系统现在可以看到 raw-bytes 需求，但仍缺正式 raw codec backend / adapter vectors。
4. Entry witness 动态参数：schema-backed payload 固定 Molecule，但 envelope 不一定需要重造。
5. Scheduler witness：public compiled scheduler policy witness bytes Molecule-only。
6. Cross-toolchain truthfulness：链上 raw bytes + codec / builder / adapter identity 未共同验证时，仍会造成事实不一致。
7. Production parity claim：没有 cycle / size / capacity 矩阵时，迁移决策空间被迫靠猜。
8. Live deployment confidence：本轮新增 BuildReport 后，compiled ELF -> live code-cell data hash 已透明；Cell.lock / Deployed.toml / `cell_data_codec_manifest` / builder manifest 仍未并入同一条 evidence chain。
9. 时间 / continuity policy：缺少一等声明式语义，审计和 builder 无法复用。

## 五、实现建议

### P0：先把证据门禁补齐

1. 本轮已经将现有 acceptance report / production gate / ELF ABI gate 统一成最小可复用的 `CellScriptBuildReport`。这不是从零新增门禁，而是把分散 evidence 收敛为一个可复现 artifact report：

```text
CellScriptBuildReport {
  schema: "cellscript-ckb-build-report-v0.20",
  name,
  kind,
  source,
  original_source,
  example,
  target_profile,
  vm_profile,
  artifact_format: "riscv64-elf",
  artifact_hash_algorithm: "ckb-blake2b256",
  deployable_elf_hash,
  artifact_sha256,
  artifact_size_bytes,
  deployment_hash_type_used_by_gate: "data1",
  verify_artifact_status,
  verify_target_profile,
  elf_entry_abi_status,
  abi_trailer_stripped,
  onchain_deployments: [
    {
      out_point,
      live_code_cell_data_hash,
      live_code_cell_data_hash_matches_artifact,
      code_cell_live,
    }
  ],
}
```

下一步需要把以下字段并入同一 row，而不是放在旁路文档里：

```text
cell_data_codec_manifest_hash
metadata_hash
cell_lock_hash
deployed_toml_hash
builder_manifest_hash
valid_carrier_result
tampered_carrier_result
cycles
tx_size
occupied_capacity
ckb_node_version_or_commit
```

2. CKB backend release gate 必须继续以「同一个 CKB-deployable artifact 贯穿到底」为核心不变量。`cellc build --target riscv64-elf --target-profile ckb` 产出的 bare ELF 现在已经绑定 compiled hash、verify-artifact、ELF entry ABI 和 live code cell `data.hash`；下一步还必须绑定：

- metadata `artifact_hash`；
- `Cell.lock` package build artifact hash；
- `Deployed.toml` artifact / deployment identity；
- valid / tampered carrier；
- BuildReport。

3. CKB backend release gate 固化为：

```text
cargo tests pass
ckb-testtool accepts valid fixture
ckb-testtool rejects tampered fixture
live parent-CKB devnet accepts deployed valid carrier
live parent-CKB devnet rejects tampered carrier
deployable artifact hash matches reported hash
live code cell data.hash == BuildReport deployable_elf_hash
```

4. 为 ELF / deploy profile 增加 regression：

- executable segment permission；
- stack write legality；
- bare ELF must not embed `CSABITR0` trailer for CKB profile；
- ELF magic 正确；
- executable `PT_LOAD` 不可写；
- `p_filesz == p_memsz`；
- `hash_type=data2` 作为显式 deployment invariant；
- `code_hash` / `data_hash` consistency；
- typed-cell commitment tamper rejection。

5. Tamper rejection gate 必须证明「合约语义拒绝」，不能只证明交易失败。每个负例应与 valid carrier 只差一个语义字段，例如 output data package commitment、type args 中的数据哈希、witness payload、CellDep out_point 或 hash_type；gate 必须记录被篡改字段、`ckb-testtool` 结果、live CKB dry-run / test-tx-pool 结果、CellScript verifier error code，并确认失败不是 capacity、missing dep、malformed tx 或 wrong script identity 引起。

### 设计 RFC：把 ABI 面拆开，继续产品化 codec manifest

不要直接把 `runtime.vm_abi.format` 改成 `raw-bytes`。本轮已经保留 CKB runtime ABI 的语义，并新增最小 `cell_data_codec_manifest`。后续 RFC 应把它扩展成可被 package、registry、builder 和 adapter 共同验证的 codec identity：

```text
codec_manifests = [
  { kind = "molecule-v1", manifest_hash = "..." },
  { kind = "raw-bytes-v1", manifest_hash = "..." }
]
```

第一阶段不要把 `ifrn-v1` 放进 compiler core。更稳的路线仍然是：

- 保留 `molecule_schema_manifest` 兼容字段；
- 保留并继续硬化 `cell_data_codec_manifest`；
- 把 `cell_data_codec_manifest.manifest_hash` 接入 `Cell.lock`、`Deployed.toml`、builder manifest 和 production evidence；
- 将 `ifrn-v1` 作为 external adapter / profile identity 进入 lockfile、builder manifest 和 package manifest；
- 对 entry witness 增加参数级 `payload_codec` metadata，而不是重造整个 witness envelope；
- 对 public scheduler policy witness 先保留 Molecule-only fail-closed，未来再单独扩展。

不要让 raw-layout 合约继续伪装成 Molecule metadata。枚举只是 metadata truthfulness 的入口，真正工作包括 codec、builder、registry、indexer、witness、adapter identity 和 test vectors。

### 长项：提供 raw layout metadata，而不是只提供 raw syscalls

这是长期语言特性，不是普通 P1 ticket。它会影响 parser、formatter、type-checker、lowering、codegen、metadata、builder 和 docs，需要单独 RFC 和 release plan。可以设计类似：

```text
#[abi(raw_bytes)]
struct NovaStateV0 {
  #[offset(0)] version: u16,
  #[offset(2)] btc_authority_hash: Hash,
  #[offset(34)] state_hash: Hash,
  #[offset(66)] policy_hash: Hash,
  #[offset(98)] latest_receipt_hash: Hash,
  #[offset(130)] nonce: u64le,
  #[offset(138)] expiry: u64le,
}
```

编译器需要生成：

- layout size；
- field offset table；
- endian 信息；
- fixed / variable segment 信息；
- non-overlap validation；
- offset + width bounds validation；
- dynamic segment bounds；
- canonical byte layout；
- hash domain；
- test vectors；
- builder codec hooks；
- verifier-side load plan；
- codec manifest hash。

这样 raw layout 不再只是手写 verifier 技巧，而是可以被 audit、builder、indexer 复用的正式 ABI。

### P1：TypeScript builder 支持 codec backend 插件

本轮已把 `cell_data_codec_manifest` 导出到 generated TypeScript builder manifest 和每个 action plan。下一步建议把 `gen-builder` 明确拆成两层：

1. action / deployment / witness planning；
2. value codec backend。

codec backend 可以是：

- Molecule；
- raw fixed bytes；
- external IFRN adapter；
- external custom codec adapter。

对 Infern 的短期路线可以先支持 external adapter：

```text
cellc gen-builder --target typescript \
  --codec-adapter infern-codec \
  --metadata target/...
```

生成的 builder 不负责实现 IFRN encoding，但必须类型化调用 adapter，并通过 test vectors 验证 byte-for-byte output。

adapter manifest 必须记录：

- adapter package name；
- version；
- content hash；
- test vector hash；
- supported codec kinds；
- fail-closed policy：非 Molecule ABI 若没有 adapter，`gen-builder` 可以生成 action plan 和 manifest，但不应宣称自己能 materialize raw cell-data bytes。

### P1：新增 raw-layout builder roundtrip gate

每个非 Molecule ABI 至少需要：

- CellScript struct value；
- expected raw bytes；
- TS encoder output；
- Rust/reference encoder output；
- on-chain `cell_data_*` verifier accepted；
- tampered field rejected；
- wrong length rejected；
- endian flip rejected；
- overlap manifest rejected；
- out-of-range offset rejected；

这会直接防止「metadata 说 A，链上读 B，builder 编 C」。

同一 gate 还应绑定 package / registry identity：`Cell.lock`、`Deployed.toml`、builder manifest 必须同时记录并校验 `metadata_hash`、`cell_data_codec_manifest_hash`、`codec_adapter_hash`。任一 mismatch 应使 registry verify 和 `gen-builder --lockfile` fail closed。

### P0 decision gate：建立真实项目 parity matrix

建议优先选三组：

1. iCKB：把 `tests/benchmarks/ickb_specs/*.cell` 明确记录为 authoritative benchmark surface，并删除旧 roadmap 中 `examples/ickb_benchmark/*.cell` 的 stale claim。
2. NovaSeal：Molecule-native profiles + `v0-mvp-skeleton` raw layout profiles。
3. Infern：六个 Rust scripts 与 CellScript spike 对比。需要先明确 owner：由 CellScript 团队、Infern 团队还是 joint spike 共同产出，否则这个 matrix 很容易停在规格层。

矩阵字段：

```text
script_name
rust_binary_hash
cellscript_binary_hash
valid_fixture_result
invalid_fixture_result
cycle_rust
cycle_cellscript
binary_size_rust
binary_size_cellscript
tx_size
occupied_capacity
deployment_profile
notes
```

这是判断 CellScript 是 production compiler 还是 audit/provenance companion 的关键证据。

### P2：把 cross-cell pattern 变成标准 fixtures

不要再笼统说 cross-cell missing。应拆成可测 pattern：

- current input previous outpoint equals witness field；
- output state references input outpoint；
- group input/output continuity；
- MetaPoint relative distance；
- source scan by outpoint；
- type hash / lock hash pairing；
- outputs and outputs-data alignment；
- occupied capacity constraints。

每个 pattern 都应有正例、负例、wrong source、wrong index、duplicate match、missing match。

### P2：声明式时间与 continuity policy

建议引入一等 policy，而不是一直手写：

```text
continuity {
  preserve owner_lock;
  monotonic nonce;
  require expiry >= env.current_timepoint;
  require updated_at > old.updated_at;
}
```

metadata 中应暴露这些 policy，供 ProofPlan、builder、audit report 和 registry verifier 使用。

### P2：修正 roadmap / docs 的 claims

需要把以下事实写清楚：

- iCKB executable specs 在 `tests/benchmarks/ickb_specs/`，且按 README 说明故意不进入 public examples tree；
- 旧 roadmap 中 `examples/ickb_benchmark/*.cell` 是 stale claim；
- 0.17/0.18 iCKB evidence 不是任意 production Rust port 证明；
- Molecule-native 是当前 first-class ABI；
- raw byte layout 是 expressible escape hatch，但 off-chain tooling support 不完整；
- live CKB smoke 是 release 前 backend gate，不应只写成外部项目经验。

## 六、对 Infern / IFRN 的建议路线

短期不建议承诺「全量 port 到 CellScript 并复用 gen-builder」。这一路线只在「Infern 六合约确实是用户描述的 raw-layout / IFRN 模式」这一前提下成立。更稳妥的顺序是：

1. 逐个 spike 六个合约的 raw-layout source expressibility。
2. 短期：对每个 cell data 类型写 hand-rolled TS/Rust encoder，先不要伪装成 Molecule。
3. 中期：把 hand-written codec 包装成 external codec adapter，并让 builder manifest 记录 adapter package、version、content hash 和 test vector hash。
4. 用 CellScript verifier 只负责链上 byte-offset / outpoint / signature / capacity 逻辑。
5. 建立 Rust original vs CellScript emitted RISC-V 的 valid/invalid result、cycles、binary size、tx size、occupied capacity 和 builder/encoder test vector matrix。
6. 如果成本和证据都可接受，再把 `raw-bytes-v1` 从最小 metadata honesty 扩展成正式 raw layout DSL / codec backend；`ifrn-v1` 先作为 adapter/profile identity，而不是第一阶段 compiler core ABI。

这把问题从「能不能 port」收窄为「六个 cell type 的 off-chain encoder 成本和 parity 风险是否可接受」。

## 七、推荐优先级

### 立刻可做

1. 已完成：保持单一 active report 文件路径，避免 `_ZH` / `_zh-cn` stale duplicate。
2. 已完成：把 `tests/benchmarks/ickb_specs/*.cell` 固定为清晰 benchmark surface，并修正 `examples/ickb_benchmark/*.cell` stale claim。
3. 已完成：把 `CellScriptBuildReport` schema 写进 `docs/CELLSCRIPT_GATE_POLICY.md`，并明确它整合现有 acceptance report / production gate / ELF ABI gate，而不是替代它们。
4. 已完成：新增 `cell_data_codec_manifest`，让 raw `LOAD_CELL_DATA` 访问声明为 `molecule+raw-bytes-v1`，并让 TypeScript builder manifest / action plan 暴露该 manifest。
5. 仍需继续：为现有 Molecule / scheduler witness / 0.18 helper / DOB-EVO audit 结论补齐 citation anchors 到稳定行号或 release-tagged 文档。

### 决策门禁

1. BuildReport + live CKB backend gate 已覆盖 compiled ELF -> live code-cell data hash；下一步要求 exact artifact 继续贯穿 metadata、Cell.lock、Deployed.toml、`cell_data_codec_manifest`、builder manifest、valid/tampered carrier。注意这是 release infra 持续成本，不只是一次 schema 变更。
2. cycle / size / behaviour parity matrix，作为 production compiler claim 的 decision gate。
3. raw-layout roundtrip test vectors，防止 metadata、builder、encoder、on-chain verifier 各说各话。
4. Issue 11 中列出的 dev-gate coverage gaps 按最便宜先做排序，优先补 fixture/gate，再改语言表面积。

### 多季度长项

1. `cell_data_codec_manifest` 与 lockfile / deployment / registry evidence 的完整绑定。
2. TypeScript builder codec backend / external adapter contract。
3. `#[abi(raw_bytes)]` / `#[offset(N)]` 语法和完整 raw layout language support。
4. live CKB devnet 持续 infra。
5. registry / indexer 对多 ABI 的正式支持。
6. 声明式 continuity / timepoint policy。
7. Infern 六合约完整 port 评估。

## 八、结论

CellScript 当前最诚实的定位应是：

> CellScript 是一个 CKB compiler，同时也是一个强 typed-cell / metadata / audit / provenance 工具链。当前已闭环的产品 claim 是本仓库内 Molecule-native typed-cell、NovaSeal/DOB source-package production readiness、audit/provenance 和 builder identity；CKB backend 已通过当前真实 devnet workload。对 raw-layout / IFRN 类合约，链上 source-level expressibility 已有基础，但 public/mainnet release、off-chain codec、builder、registry/indexer metadata 和真实项目 parity evidence 仍应按独立证据链推进。

这不是否定 CellScript 的价值。相反，这次改进闭环把问题从泛泛的「能不能生产」压缩成了几个可以落地的工程项。本轮已补上最小 exact-artifact BuildReport、live backend data-hash linkage、`cell_data_codec_manifest`、builder manifest 暴露、variable-length packed hash、BIP340 IPC diagnostic 和 planned-profile live devnet evidence；剩余关键项是 public/mainnet external attestations、builder codec adapter、raw-layout roundtrip vectors、cycle/size matrix，以及把 Cell.lock / Deployed.toml / carrier evidence 接进同一 identity chain。把这些继续补上，CellScript 的 production CKB compiler claim 就能从当前 source-package readiness 继续扩展到公开部署和外部生态证据。
