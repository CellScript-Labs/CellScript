# CellScript 0.15 发布前全面审计与生产验收严格加固方案

**版本**: 0.15.0  
**状态**: 发布门控草案  
**更新日期**: 2026-05-17

---

## 目录

1. [总体原则](#总体原则)
2. [审计阶段一：编译器核心硬化](#审计阶段一编译器核心硬化)
3. [审计阶段二：语法组合预飞行](#审计阶段二语法组合预飞行)
4. [审计阶段三：CKB 生产证据门控](#审计阶段三ckb-生产证据门控)
5. [审计阶段四：工具链与 LSP 完整性](#审计阶段四工具链与-lsp-完整性)
6. [审计阶段五：文档与元数据一致性](#审计阶段五文档与元数据一致性)
7. [生产验收严格加固方案](#生产验收严格加固方案)
8. [签核清单](#签核清单)

---

## 总体原则

0.15 是 CellScript 的 **scoped-invariant + Covenant ProofPlan** 里程碑。发布前审计遵循以下原则：

- **编译器证据 ≠ 链上证据**：`cargo test` 通过和 `cellc verify-artifact` 通过只证明编译器层面的一致性，不代表 CKB 交易可以被实际构建、dry-run 和提交。
- **Metadata-only 不是失败，但也不是证明**：聚合不变量原语（`assert_sum`、`assert_conserved` 等）在 0.15 中仍停留在 `gap:metadata-only`。审计时必须明确区分哪些义务已被可执行代码覆盖，哪些仍需要 builder/indexer 证据来闭合。
- **Fail-closed 是正确行为，但必须在元数据中可见**：任何因为 lowering 未完成而 fail-closed 的路径，必须在 ProofPlan 中留下 `runtime-required` 或 `fail_closed` 记录。
- **Primitive-strict 是默认生产姿态**：所有捆绑示例和发布声明必须使用 `--primitive-strict=0.15`。

---

## 审计阶段一：编译器核心硬化

### 1.1 Rust 工程健康检查

```bash
# 格式化检查
cargo fmt --all -- --check

# 无警告编译
cargo check --locked -p cellscript --all-targets
cargo clippy --locked -p cellscript --all-targets -- -D warnings

# 差异检查
git diff --check
```

**Prompt**：
> 作为 Rust 审计员，验证 `cellscript` crate 满足以下条件：
> - `Cargo.toml` 版本、`Cargo.lock` 版本、`CHANGELOG.md` 首行版本、VS Code `package.json` 版本、`src/lib.rs` 中 `VERSION` 常量完全一致。
> - 不存在未提交的格式化差异。
> - Clippy 在 `-D warnings` 下零警告通过。
> - 所有 `TODO(0.15)`、`FIXME(0.15)`、`HACK(0.15)` 注释已被清理或转为 roadmap issue。

### 1.2 单元测试与集成测试

```bash
# 核心库测试
cargo test --locked -p cellscript

# 0.15 专属聚焦测试
cargo test --locked -p cellscript proof_plan --lib
cargo test --locked -p cellscript aggregate_invariant --lib
cargo test --locked -p cellscript --lib -- compile_identity compile_unique
cargo test --locked -p cellscript explain_proof --test cli
cargo test --locked -p cellscript docgen --lib
```

**Prompt**：
> 作为编译器测试审计员，确认：
> - `proof_plan` 测试覆盖 invariant trigger、scope、reads、relation_checks、coverage_status 的元数据正确性。
> - `aggregate_invariant` 测试覆盖五种原语（`assert_sum`、`assert_conserved`、`assert_delta`、`assert_distinct`、`assert_singleton`）的解析、类型检查、格式化、IR lowering。
> - `compile_identity` 和 `compile_unique` 测试覆盖四种策略（`ckb_type_id`、`field(...)`、`script_args`、`singleton_type`）的解析、元数据、codegen 锚点。
> - `explain_proof` CLI 测试覆盖人类可读输出和 JSON 输出的 schema 稳定性。
> - 不存在被忽略的测试（`#[ignore]`）除非有对应的 roadmap issue 编号。

### 1.3 后端形状稳定性

```bash
# 生成后端审计产物（ELF + metadata）
mkdir -p target/cellscript-audit
cargo run --locked -p cellscript -- \
  examples/token.cell \
  --target riscv64-elf --target-profile ckb --primitive-strict 0.15 \
  -o target/cellscript-audit/token.elf
```

**Expected artifacts**：
- `target/cellscript-audit/token.elf`
- `target/cellscript-audit/token.elf.meta.json`

**Backend shape evidence is collected from**：
1. ELF header validation (`xxd -l 4` → `\x7fELF`).
2. Generated `.meta.json` inspection (runtime features, verifier obligations, fail-closed list).
3. Targeted source inspection of RISC-V emit paths (`emit_create_unique`, `emit_replace_unique`, `emit_destroy`).
4. Negative confirmation that unsupported policies remain `runtime-required` rather than being over-lowered.

**Prompt**：
> 作为后端审计员，验证：
> - 生成的 RISC-V ELF 以 `\x7fELF` 开头。
> - 每种新 IR instruction（`CreateUnique`、`ReplaceUnique`、`Destroy` with policy）都在 codegen 中有对应的 emit 路径。
> - `create_unique` 对每个 identity policy 都发射了正确的运行时锚点（field bytes / LockHash / TypeHash）。
> - `destroy_unique`（`identity = type_id`）发射了可执行的 output TypeHash absence scan，而不是 fail-closed。
> - `destroy_instance` 和 `burn_amount` 明确标记为 metadata-visible runtime-required，不发射 over-broad 的 TypeHash scan。
> - 没有新增的未处理 IR pattern 导致 panic 或默认分支。

**Decision**：Do not add `--backend-shape-report` for 0.15. The audit document was ahead of the current CLI surface. A dedicated backend-shape report is a valuable 0.16+ compiler evidence feature, not a 0.15 release blocker.

---

## 审计阶段二：语法组合预飞行

### 2.1 快速门控（本地预推送）

```bash
./scripts/cellscript_syntax_combo_audit.sh quick
```

**Prompt**：
> 作为语法组合审计员，运行 quick 模式并验证：
> - 生成案例数 ≥ 17，接受数 ≥ 8，拒绝数 ≥ 9。
> - 必须覆盖以下 regression seeds：
>   - `tests/syntax_combo/seeds/legacy-transfer-capability.cell`
>   - `tests/syntax_combo/seeds/require-block-lifecycle.cell`
> - 报告目录 `target/syntax-combo-audit/` 存在且包含 compact 报告。
> - 没有 formatter 崩溃或解析器 panic。

### 2.2 CI 门控（PR 合并前）

```bash
./scripts/cellscript_syntax_combo_audit.sh ci
```

**Prompt**：
> 作为 CI 审计员，运行 ci 模式并验证：
> - 生成案例数 ≥ 37，接受数 ≥ 22，拒绝数 ≥ 15。
> - 必须覆盖 `matrix.toml` 中列出的所有 required origins：
>   - `matrix:continuity/std-cell`
>   - `matrix:lifecycle/proof/control-flow`
>   - `matrix:lifecycle/proof/local-binding`
>   - `matrix:lock/source-qualifier`
>   - `matrix:receipt/metadata`
>   - `matrix:receipt/proof`
>   - `matrix:reject/metadata`
>   - `matrix:reject/proof-purity`
>   - `matrix:reject/stdlib-lifecycle`
>   - `matrix:stdlib-lifecycle/metadata`
>   - `matrix:stdlib-lifecycle/proof`
> - 所有拒绝案例都有明确的诊断信息（不是 panic 或内部错误）。
> - 所有接受案例都能通过 `cellc fmt` 而不改变语义。

### 2.3 深度门控（发布前最终验证）

```bash
./scripts/cellscript_syntax_combo_audit.sh deep
```

**Prompt**：
> 作为发布前深度审计员，运行 deep 模式并验证：
> - 覆盖所有已知的语法轴交叉点，特别是 0.15 新增特性与其他语法的交互：
>   - `invariant` + `action` + `transition`
>   - `create_unique` / `replace_unique` + `flow`
>   - `destroy_singleton_type` / `destroy_unique` / `destroy_instance` / `burn_amount` + `where` 分支
>   - `assert_sum` / `assert_conserved` 等 + 不同 `trigger`/`scope` 组合
>   - `identity(...)` + `has store` + `with_default_hash_type`
> - 没有隐藏的语法组合导致编译器 silently producing wrong metadata。

---

## 审计阶段三：CKB 生产证据门控

### 3.1 编译门控（Compiler Gate）

```bash
# 单个文件编译验证
for f in examples/*.cell; do
  echo "==> $f"
  cellc "$f" --target riscv64-elf --target-profile ckb --primitive-strict 0.15 \
    -o "/tmp/$(basename "$f" .cell).elf"
  cellc verify-artifact "/tmp/$(basename "$f" .cell).elf" \
    --expect-target-profile ckb --verify-sources
done

# 语言示例编译（非生产，但需通过编译器测试）
for f in examples/language/*.cell; do
  echo "==> $f"
  cellc "$f" --target riscv64-elf --target-profile ckb --primitive-strict 0.15 \
    -o "/tmp/$(basename "$f" .cell).elf" || true
done
```

**Prompt**：
> 作为 CKB 编译审计员，验证：
> - 7 个捆绑业务示例（`token`, `nft`, `timelock`, `multisig`, `vesting`, `amm_pool`, `launch`）全部以 `--primitive-strict=0.15` 成功编译为 ELF。
> - 每个 ELF 都有对应的 `.meta.json` sidecar。
> - `verify-artifact` 对每个捆绑示例都通过，且 `target_profile` 确实是 `ckb`。
> - `examples/registry.cell` 和 `examples/language/*.cell` 作为非生产语言示例，至少不导致编译器 panic。
> - 严格模式下，`has transfer` 和 `has destroy` 被正确拒绝（CS0150/CS0151）。
> - 所有资源、共享状态、收据都声明了 `has store`（除了明确设计为 ephemeral 的 receipt）。

### 3.2 CKB 发布门控脚本

```bash
# 快速门控（开发循环）
./scripts/cellscript_ckb_release_gate.sh

# 完整门控（发布前最终验证）
./scripts/cellscript_ckb_release_gate.sh full
```

**Prompt**：
> 作为 CKB 发布门控审计员，运行 `full` 模式并验证：
> - 脚本首先运行 syntax-combination CI preflight，且 preflight 通过。
> - 所有捆绑示例都在 CKB  profile 下以 `--primitive-strict=0.15` 编译。
> - 存在 builder-backed 的 action runs（有效交易 dry-run）。
> - 存在 builder-backed 的 lock valid-spend 和 invalid-spend 矩阵。
> - 有效交易已被提交到本地 CKB 链。
> - 畸形交易因非策略/非容量原因被拒绝。
> - 保留了 cycles、transaction size、occupied-capacity 证据。
> - 不存在 under-capacity 输出。
> - 捆绑示例已被部署。
> - **最终生产硬化门控（final production hardening gate）明确记录为 passed**。

### 3.3 生产证据验证

```bash
python3 scripts/validate_ckb_cellscript_production_evidence.py \
  target/ckb-cellscript-acceptance/report.json
```

**Prompt**：
> 作为生产证据审计员，验证 JSON 报告满足：
> - `acceptance_mode` == `"production"`
> - `status` == `"passed"`
> - `production_ready` == `true`
> - `bundled_examples_count` == 7
> - `bundled_examples_exact_order` 匹配：
>   `["amm_pool.cell", "launch.cell", "multisig.cell", "nft.cell", "timelock.cell", "token.cell", "vesting.cell"]`
> - `original_scoped_action_count` == 43
> - `original_scoped_lock_count` == 预期锁总数
> - `original_scoped_action_fail_closed_count` == 0
> - `original_scoped_lock_fail_closed_count` == 0
> - `strict_original_ckb_compile_policy_fail_closed` == `[]`
> - `production_gate.status` == `"passed"`
> - `production_gate.failures` == `[]`
> - `ckb_business_coverage.strict_compile_coverage_complete` == `true`
> - `ckb_business_coverage.expected_fail_closed_action_count` == 0
> - `ckb_business_coverage.expected_fail_closed_lock_count` == 0
> - 每个 `ACTION_RUN_KEYS` 中的动作列表与预期完全一致，无重复。
> - Lock spend 矩阵覆盖所有预期锁：`multisig`、`nft`、`timelock`、`vesting`。

### 3.4 ProofPlan 元数据审计

```bash
# 对关键示例运行 explain-proof
cargo run --locked -p cellscript -- explain-proof \
  examples/token.cell --target riscv64-elf --target-profile ckb --json \
  > /tmp/token-proof-plan.json

cargo run --locked -p cellscript -- explain-proof \
  examples/language/v0_15_scoped_invariant.cell \
  --target riscv64-elf --target-profile ckb --json \
  > /tmp/invariant-proof-plan.json
```

**Prompt**：
> 作为 ProofPlan 审计员，验证：
> - `cellc explain-proof` 对每个捆绑示例都能成功输出 JSON。
> - 包含 `invariant` 的示例（如 `v0_15_scoped_invariant.cell`）的 ProofPlan 记录包含：
>   - 正确的 `trigger`、`scope`、`reads`
>   - `relation_checks` 列出聚合原语和关系
>   - `codegen_coverage_status` 明确标记为 `gap:metadata-only`
>   - `on_chain_checked: no`
>   - `builder_assumption` 列出需要 builder 闭合的元数据义务
> - `--deny-runtime-obligations` 会拒绝包含未闭合 metadata-only invariant 的包。
> - `lock_group + transaction` 组合产生明确的诊断警告（因为 lock group 触发器无法保证 transaction-wide 视图的完全检查）。

### 3.5 有状态场景测试

```bash
./scripts/cellscript_ckb_stateful_scenarios.sh
```

**Prompt**：
> 作为有状态场景审计员，验证：
> - 多步协议流程（如 vesting grant → claim → revoke）在本地 CKB 链上按顺序执行成功。
> - 中间状态 Cell 在每个步骤后都可被正确索引。
> - 违反 invariant 的交易（如重复 claim 同一 vesting grant）被正确拒绝。
> - 时间锁示例的 `env::current_timepoint()` 映射到 HeaderDep#0 epoch number，而非 Unix 时间戳。

---

## 审计阶段四：工具链与 LSP 完整性

### 4.1 工具链发布边界验证

```bash
python3 scripts/validate_cellscript_tooling_release.py
```

**Prompt**：
> 作为工具链审计员，验证：
> - `Cargo.toml` 版本 == `Cargo.lock` 版本 == VS Code `package.json` 版本 == `CHANGELOG.md` 首行版本。
> - `src/lib.rs` 中 `VERSION` 使用 `env!("CARGO_PKG_VERSION")`。
> - `src/main.rs` 中 `#[command(version = cellscript::VERSION)]`。
> - VS Code 扩展包含完整的 LSP 客户端配置（`--lsp`、`TransportKind.stdio`）。
> - VS Code 扩展包含 `cellscript.showConstraints` 和 `cellscript.showProductionReport` 命令。
> - 包管理器正确拒绝 registry 依赖（fail-closed）。
> - `cellc init`、`cellc build`、`cellc check`、`cellc fmt`、`cellc doc`、`cellc add --path`、`cellc remove`、`cellc install --path`、`cellc update` 全部可用。

### 4.2 LSP 功能验证

```bash
cd editors/vscode-cellscript
npm install
npm run validate
npm run package
```

**Prompt**：
> 作为 LSP 审计员，验证：
> - `cellc --lsp` 启动 JSON-RPC over stdio 服务器且不崩溃。
> - LSP 支持以下功能：diagnostics、hover、go-to-definition、find-references、rename、document symbols、document highlight、signature help、folding ranges、selection ranges、formatting、code actions。
> - 对 0.15 新语法的支持：
>   - `invariant` 声明出现在 document symbols 中。
>   - `assert_sum`、`assert_conserved` 等出现在 completions 中。
>   - `create_unique`、`replace_unique`、`destroy_singleton_type`、`destroy_unique`、`destroy_instance`、`burn_amount` 出现在 completions 中。
>   - `identity(ckb_type_id)`、`identity(field(...))` 等出现在 hover 信息中。
> - Formatter 对 invariant 块、aggregate assertions、identity declarations 的格式化结果与 parser 期望一致。
> - Docgen 对 invariant 和 identity policy 的输出包含在 `cellc doc --json` 中。

---

## 审计阶段五：文档与元数据一致性

### 5.1 Wiki 文档审计

**Prompt**：
> 作为文档审计员，逐页验证 wiki 文档与代码现状一致：
> - `Tutorial-02-Language-Basics.md`：
>   - `receipt T` 的语法 checklist 不强制 `has store`。
>   - `protected` 和 `lock_args` 明确标注为 lock-only。
>   - 0.15 内核效果列表完整：`create`, `consume`, `replace`, `burn`, `relock`, `retarget_type`, `read_ref`。
> - `Tutorial-03-Resources-and-Cell-Effects.md`：
>   - 四种显式销毁策略的描述与 codegen 实际行为一致。
>   - `destroy_unique(value, identity = type_id)` 确实发射可执行的 TypeHash absence scan。
>   - `destroy_instance` 和 `burn_amount` 标记为 runtime-required，不发射 over-broad scan。
> - `Tutorial-10-Standard-Library.md`：
>   - 版本标签为 `0.15` 而非 `0.13.2`。
>   - `std::lifecycle::transfer`、`std::receipt::claim`、`std::lifecycle::settle` 的签名与类型检查器一致。
> - `Tutorial-11-Scoped-Invariants-and-ProofPlan.md`：
>   - `explain-proof` 示例输出与当前编译器实际输出格式一致（包含 `checked_partial`、`fail_closed`、`macro_provenance_records` 等）。
>   - 包含 `assert_invariant` 的说明。
>   - 明确声明 aggregate primitives 是 metadata-only。

### 5.2 发布说明与路线图一致性

```bash
# 验证 roadmap 文档存在且内容正确
rg --fixed-strings "0.15" roadmap/CELLSCRIPT_0_15_ROADMAP.md
docs/CELLSCRIPT_0_15_RELEASE_NOTES_DRAFT.md
```

**Prompt**：
> 作为发布文档审计员，验证：
> - `docs/CELLSCRIPT_0_15_RELEASE_NOTES_DRAFT.md` 包含所有 0.15 核心特性的准确描述。
> - `roadmap/CELLSCRIPT_0_15_ROADMAP.md` 中的 "Implemented In This Branch" 表格与代码实际状态一致。
> - "Boundaries" 章节准确描述了 metadata-only 的边界、lock_group+transaction 的风险、aggregate primitives 的 fixed-width 限制。
> - 不存在已实现但被标记为 "Not Implemented" 的特性，也不存在未实现但被标记为 "Implemented" 的特性。
> - `CHANGELOG.md` 包含从 0.14 到 0.15 的所有用户可见变更。

### 5.3 Molecule Schema 与元数据契约

```bash
# 生成 schema manifest 报告
mkdir -p target/cellscript-schema-manifest
cargo run --locked -p cellscript -- \
  examples/token.cell --target riscv64-elf --target-profile ckb --primitive-strict 0.15 \
  --schema-manifest-report target/cellscript-schema-manifest/schema-manifest-report-main.json
```

**Prompt**：
> 作为 schema 审计员，验证：
> - 所有 persistent 类型（resource、shared、receipt）的 Molecule schema 在元数据中完整声明。
> - `identity_policy` 字段出现在 `TypeMetadata` 中（非 `none` 时可见）。
> - `type_hash` 相关元数据字段已按 0.15 规范重命名：
>   - `type_hash-absence` → `ckb_type_script_hash-absence`
>   - `type_hash-preservation` → `ckb_type_script_hash-preservation`
>   - `lock_hash-preservation` → `ckb_lock_script_hash-preservation`
> - `entry_abi` 和 `entry_witness` 元数据包含 invariant 相关的 witness 布局。

---

## 生产验收严格加固方案

### 方案概述

本方案定义从 **编译器证据** → **Builder 证据** → **链上证据** 的三层加固流程。任何声称 "production-ready" 的 0.15 发布必须完成全部三层。

### Layer 1：编译器证据（Compiler Evidence）

**目标**：证明源代码、编译产物、元数据三者一致，且编译器本身健康。

| 检查项 | 命令/方法 | 通过标准 |
|---|---|---|
| 格式化零差异 | `cargo fmt --all -- --check` | 零差异 |
| 零警告编译 | `cargo clippy --locked -p cellscript --all-targets -- -D warnings` | 零警告 |
| 全量测试通过 | `cargo test --locked -p cellscript` | 全部通过 |
| 语法组合预飞行 | `./scripts/cellscript_syntax_combo_audit.sh ci` | 生成 ≥37 案例，接受 ≥22，拒绝 ≥15 |
| 工具链边界验证 | `python3 scripts/validate_cellscript_tooling_release.py` | 输出 "valid CellScript tooling release boundary" |
| 版本一致性 | 手动检查 | `Cargo.toml` == `Cargo.lock` == `package.json` == `CHANGELOG.md` == `src/lib.rs` |

**加固要求**：
- 任何 `cargo test` 失败都是发布阻塞项（blocking）。
- 任何语法组合审计中的 panic 或内部错误都是阻塞项。
- Clippy warning 在 `-D warnings` 下必须为零；允许的例外必须记录在有编号的 issue 中。

### Layer 2：Builder 证据（Builder Evidence）

**目标**：证明编译产物可以被实际用于构建 CKB 交易，且交易能通过 dry-run。

| 检查项 | 命令/方法 | 通过标准 |
|---|---|---|
| 捆绑示例 ELF 编译 | `cellc examples/*.cell --target riscv64-elf --target-profile ckb --primitive-strict 0.15` | 7 个全部成功 |
| Artifact 验证 | `cellc verify-artifact ... --expect-target-profile ckb --verify-sources --production` | 全部通过 |
| 动作运行矩阵 | `./scripts/cellscript_ckb_release_gate.sh full` | 43 个 scoped actions 全部有 dry-run 证据 |
| 锁支出矩阵 | `./scripts/cellscript_ckb_release_gate.sh full` | 每个锁都有 valid-spend + invalid-spend 证据 |
| 容量证据 | 检查报告 | occupied-capacity 测量值 ≥ 声明的 capacity floor |
| 交易尺寸证据 | 检查报告 | serialized tx size 在 CKB 限制内 |
| Cycles 测量 | 检查报告 | 每个动作/锁的 cycles 被记录 |

**加固要求**：
- 所有 builder-generated 交易必须通过 `ckb-jsonrpc` 的 `dry_run_transaction`。
- malformed 交易（错误 witness、错误 lock、under-capacity）必须被正确拒绝，且拒绝原因不是 "policy/capacity" 误判。
- 每个 bundled example 必须在本地 devnet 上完成至少一次完整部署。

### Layer 3：链上证据（Chain Evidence）

**目标**：证明交易在真实的 CKB 本地链上可以被提交，且状态转换正确。

| 检查项 | 命令/方法 | 通过标准 |
|---|---|---|
| 有效交易提交 | 检查报告 | valid transactions committed == true |
| 无效交易拒绝 | 检查报告 | malformed transactions rejected == true |
| 无 under-capacity | 检查报告 | `no_under_capacity_outputs` == true |
| 状态可索引 | 手动/脚本 | 多步流程的中间状态 Cell 可被 indexer 查询 |
| 生产硬化门控 | 检查报告 | `final_production_hardening_gate` == "passed" |

**加固要求**：
- 最终生产硬化门控（final production hardening gate）是 **不可跳过的强制检查**。
- 发布声明中必须明确区分：哪些是 "compiler evidence"，哪些是 "builder evidence"，哪些是 "chain evidence"。
- 任何标记为 `runtime-required` 或 `gap:metadata-only` 的 ProofPlan 义务，必须在发布说明中列出，并指明由谁负责闭合（action checks、lock code、builder policy、future lowering）。

### 加固执行脚本（一键运行）

```bash
#!/usr/bin/env bash
# cellscript-0.15-full-hardening.sh
# 0.15 发布前全面硬化脚本

set -euo pipefail
ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

echo "=== Layer 1: Compiler Evidence ==="
cargo fmt --all -- --check
cargo check --locked -p cellscript --all-targets
cargo clippy --locked -p cellscript --all-targets -- -D warnings
cargo test --locked -p cellscript
python3 scripts/validate_cellscript_tooling_release.py

echo "=== Layer 2: Syntax Combo Preflight ==="
./scripts/cellscript_syntax_combo_audit.sh ci

echo "=== Layer 3: CKB Release Gate ==="
./scripts/cellscript_ckb_release_gate.sh full

echo "=== Layer 4: Production Evidence Validation ==="
python3 scripts/validate_ckb_cellscript_production_evidence.py \
  target/ckb-cellscript-acceptance/report.json

echo "=== Layer 5: ProofPlan Audit ==="
for f in examples/*.cell; do
  cargo run --locked -p cellscript -- explain-proof "$f" \
    --target riscv64-elf --target-profile ckb --json \
    > "/tmp/proof-plan-$(basename "$f" .cell).json"
done

echo "=== All Hardening Layers Passed ==="
```

---

### Mutation / Negative Evidence Gate

For each bundled example, the release gate must include at least one semantic mutation test per protected dimension:

- ownership mutation
- amount/accounting mutation
- identity mutation
- witness/action selector mutation
- lock authorisation mutation
- type/lock hash preservation mutation
- capacity floor mutation
- duplicate/replay mutation where applicable

A passing positive transaction without corresponding negative evidence is not sufficient release evidence.
**发布阻塞规则**：
1. 任何 Layer 1 检查失败 → **禁止发布**。
2. 任何 bundled example 在 `--primitive-strict=0.15` 下编译失败 → **禁止发布**。
3. 任何 `fail_closed` 计数不为零 → **禁止发布**。
4. 任何 lock 缺少 valid-spend 或 invalid-spend 证据 → **禁止发布**。
5. 最终生产硬化门控未明确记录为 passed → **禁止发布**。
6. 存在未记录的 `runtime-required` ProofPlan 义务 → **禁止发布**（必须在 release notes 中列出）。
