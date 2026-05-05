# 用 iCKB 检验 CellScript 成熟度的方法论

本文档面向 iCKB 作者，解释 `research/protocol-equivalence` 分支为什么选择 iCKB
作为 CellScript 0.17 的成熟度 benchmark，以及这个分支如何避免把部分证据夸大成
生产等价声明。

核心结论先放在前面：

- 这个分支不是在声明 CellScript 已经完整复刻或替代 iCKB。
- iCKB 被用作高难度、真实 CKB 协议样本，用来检验 CellScript 是否能表达、降低并
  执行 CKB 原生约束。
- 当前 selected matrix 已经升级为 `EXECUTED_CKB_VM_DIFF` / `PROVEN`，包含
  75 条 original-vs-CellScript CKB VM 双侧差分行，active `MODEL` 行和 active
  非可执行假设均为 0。
- 14 条 CellScript-only VM 行、8 条 original-side VM 行和 3 条 legacy assumption
  已移入 supporting / retired evidence，不参与生产等价 claim。
- 这个 `PROVEN` 结论只覆盖 selected executed matrix；真实 owner-auth witness、
  DAO redeem aggregate accounting、通用聚合降低、byte-accurate receipt decoding
  和更完整的 production evidence manifest closure 仍是后续扩展方向。

## 为什么选 iCKB

iCKB 对 CellScript 是一个合适的成熟度压力测试，不是因为它方便，而是因为它覆盖了
CKB 合约语言最容易出错的边界：

- DAO header lineage、accumulated rate、since/maturity 和容量补偿；
- receipt、deposit、withdrawal request、xUDT amount 之间的跨 cell accounting；
- xUDT owner-mode args 和 script identity 绑定；
- Limit Order 的资产守恒、比例边界和 master OutPoint/MetaPoint 关系；
- Owned-Owner 的 lock/type 双角色、相对位置、data rule 和 script role 约束；
- witness/Molecule 字节解析、malformed 输入 fail-closed、header dep 缺失拒绝；
- 正向路径和 adversarial negative path 同时存在。

这些特性会同时压到 CellScript 的语言设计、类型系统、IR、RISC-V codegen、运行时
helper、CKB syscall 使用、测试 fixture 和证据清单。换句话说，如果 CellScript 只
能编译 toy contract，它过不了 iCKB；如果它能逐步解释清楚 iCKB 的这些边界，就说明
语言正在接近真实 CKB 协议工程所需的成熟度。

## 成熟度不是一个数字

这个分支把“成熟度”拆成四个层次，而不是使用一个含混的百分比：

1. **表达能力**：CellScript 能否用协议中立的语言表面表达该类约束。
2. **可执行降低**：这些约束是否真的降低到 CKB VM 可执行的 RISC-V/ELF，而不是只停在
   metadata、ProofPlan 或 JSON model。
3. **双侧差分证据**：原始 iCKB ELF 和生成的 CellScript ELF 是否在同一个归一化场景
   中执行，并且 pass/fail 状态一致。
4. **生产声明门禁**：证据是否完整到足以声明 selected subset 的 production
   equivalence。

当前状态可以概括为：

- 选定 normalized fixtures 的差分执行矩阵已经通过；
- `rows` 只包含双侧 CKB VM 差分执行证据；
- 分支当前为 `equivalence_status = PROVEN`，但只针对选定执行矩阵；
- 任何新增生产等价行都必须先通过 `tests/benchmarks/ickb_diff/matrix.json` 和
  `tests/ickb_diff.rs` 的同等级门禁。

## 证据等级

矩阵中的每一行都必须选择明确的证据等级：

| 等级 | 含义 | 允许的说法 |
|---|---|---|
| `MODEL` | 只有模型或 fixture 语义，没有真实 CKB VM 双侧执行 | model-level、iCKB-style、partial |
| `CELL_SCRIPT_CKB_VM_EXECUTED` | 生成的 CellScript ELF 在 CKB VM/testtool 中执行过，但没有原始 iCKB 同场景侧 | CellScript VM evidence、precursor evidence |
| `ORIGINAL_ICKB_CKB_VM_EXECUTED` | 原始 iCKB ELF 在 CKB VM/testtool 中执行过，但没有 CellScript 同场景侧 | original-side VM evidence |
| `DIFFERENTIAL_CKB_VM_EXECUTED` | 原始 iCKB ELF 与 CellScript ELF 在同一个归一化夹具上双侧执行，pass/fail 一致 | executed differential subset |
| `PROVEN` | 选定矩阵内所有场景都有完整执行证据，且所有 blocker 清空 | production-equivalent for selected executed subset |

当前矩阵已经升级到 `EXECUTED_CKB_VM_DIFF` / `PROVEN`，因为选定 `rows`
内所有场景都有双侧执行、完整 execution object 和匹配 pass/fail 状态。未形成双侧
差分的单侧 VM 行保留为 `supporting_evidence`，不会参与等价声明。

## 归一化差分方法

原始 iCKB ELF 和 CellScript 生成 ELF 的 code hash 必然不同，因此这个分支不要求
两边交易字节完全相同。比较标准是“同一个归一化语义场景”：

- 语义输入相同；
- input/output capacity 相同；
- output data 相同；
- cell deps、header deps、witnesses 在被测语义上相同；
- DAO/xUDT 依赖在测试环境允许范围内等价；
- 只有 script-under-test 的 code cell 和 script hash 可以不同；
- 双方都在 CKB VM/testtool 中执行；
- 有效场景必须 pass/pass；
- 无效场景必须 fail/fail；
- reject 行必须有命名 failure mode。

行级 evidence object 需要记录：

- normalized fixture hash；
- transaction context hash；
- 原始 iCKB 二进制 SHA-256；
- 生成 CellScript artifact SHA-256；
- CKB VM 或 ckb-testtool 版本；
- 双方 exit code、status、cycles；
- tx size、occupied capacity、fee；
- reject 行的 failure mode；
- `statuses_match = true`。

这意味着“等价”不靠人工描述，也不靠只看编译结果，而是靠可重复的执行证据。

## iCKB 专用逻辑的隔离原则

这个分支有一个重要工程边界：iCKB 专用逻辑只允许存在于
`tests/benchmarks/`、`tests/support/` 和 `docs/0.17/`。

`src/` 里只能加入协议中立的 CKB/CellScript 能力，例如：

- SourceView；
- HeaderDep；
- DAO accumulated-rate/header-lineage 读取；
- CKB since/maturity 编码与检查；
- xUDT group amount helper；
- script identity/args read-check helper；
- OutPoint/MetaPoint helper；
- witness/Molecule parser；
- fail-closed runtime error ABI。

不能为了让 iCKB benchmark 更容易通过，就把 iCKB receipt byte layout、deposit/receipt
pairing、mint-sum recomputation 或 owner-auth 特例硬塞进通用语言。这样做会让
CellScript 变成“内置 iCKB 模板”，反而削弱它作为通用 CKB 合约语言的可信度。

因此，iCKB 在这里的角色是校准器：

- 如果某个缺口是 CKB 通用能力缺口，就推进到 `src/`。
- 如果某个缺口是 iCKB 协议特定 layout，就留在 benchmark/fixture 层。
- 如果某个语义暂时无法执行，就登记为 blocker，而不是伪装成已证明。

## 当前覆盖的语义族

当前 active matrix 已经把很多早期 model-only 项升级成真实 VM 执行证据，主要包括：

- deposit phase 1：有效 deposit、capacity 下界拒绝、capacity 上界拒绝、receipt
  without deposit、duplicate receipt output；
- mint from receipt：有效 mint、amount inflation/deflation、wrong xUDT args、wrong
  accumulated rate、missing header dep、malformed receipt data；
- receipt group：two-receipt exact mint、over-mint、under-mint、missing header、
  wrong accumulated rate、wrong xUDT binding、first/second malformed receipt data；
- DAO phase-2 withdrawal：mature/immature since、max capacity、over-withdraw、
  deposit/withdraw rate adjusted max、deposit/withdraw rate adjusted over-withdraw、
  wrong deposit/withdraw accumulated rate、missing withdraw/deposit header、wrong
  header index、wrong committed header、deposit-data input、malformed input data、
  missing/empty/short/long witness input_type；
- Limit Order：CKB-to-UDT 和 UDT-to-CKB 两个方向的 valid、min-match boundary、
  underpayment、wrong asset、insufficient match、no paid out、UDT decreased；
- Owned-Owner：input/output valid pairing、relative mismatch、missing/duplicate
  owner/owned、script misuse、non-withdrawal request、owner data length mismatch、
  related type-hash mismatch、related data-rule mismatch；
- CellScript-only precursor：LOAD_SCRIPT_HASH、LOAD_HEADER、DAO data classifier、
  occupied capacity、CellDep data_size、DAO mature/immature relative-since、
  WitnessArgs/Molecule malformed 路径；
- original-side precursor：原始 iCKB Logic、Limit Order、Owned-Owner 和未修改 DAO
  ELF 的多个单侧 CKB VM 场景。

这些证据说明 CellScript 已经不只是“写得像 iCKB”，而是在大量选定场景里生成了能在
CKB VM 中执行、并与原始脚本 pass/fail 一致的代码。

## 不能外推到未选定场景的原因

当前可以声明 selected executed matrix 的 production equivalence，但不能把这个结论
外推成“完整复刻所有 iCKB 行为”。主要边界如下：

- **真实 owner-auth witness**：WitnessArgs/Molecule 解析器已经有协议中立 VM 回归，
  但 iCKB 真实授权字节和 owner-auth 语义还需要进入 production fixture。
- **DAO redeem aggregate accounting**：phase-2 withdrawal 的 maturity/header/data
  边界、capacity compensation 和选定 DAO rate recomputation 已有双侧差分，但完整
  多输入 redeem accounting 仍需继续补强。
- **通用聚合降低**：xUDT group amount helper 和若干 fixture-bound aggregate probe
  已经可执行，但通用 `assert_sum` / `assert_delta` 自动 lowering 还没有完整覆盖
  receipt/deposit/DAO-rate accounting。
- **byte-accurate receipt decoding**：已有 malformed data 边界证据，但完整 receipt
  字节结构解码还不是通用语言能力。
- **production manifest closure 扩展**：selected matrix 已有必要 evidence
  manifest，但新增场景仍必须补齐 binary hash、fixture hash、tx context、capacity、
  fee、deployment/cell dep 证据，才能进入 selected rows。
- **legacy assumption 退役纪律**：duplicate receipt-id、synthetic wrong-owner
  resource fields、synthetic current-epoch immature redeem 已移入 retired assumptions，
  不再作为 active `MODEL` row 计数；未来若重新覆盖相关语义，必须以双侧 VM 差分
  行进入矩阵。

另外，一等公民 `Script` API 明确移入 0.18，不再被列为 0.17 blocker。现有
helper-level Script support 已经覆盖 active iCKB matrix 需要的 identity、args
binding、role 和 filtered scan。完整 arbitrary `Script` constructor 会把范围扩大到
deploy manifest resolution、TYPE_ID construction、script-hash synthesis 和 cell dep
solving；这些应后置为 0.18 的只读 `ScriptRef` / `ScriptArgs` 收束层，而不是阻塞
当前 0.17 证据闭环。

## 这个方法如何暴露 CellScript 的真实成熟度

这个分支不是从“CellScript 应该支持什么语法”倒推测试，而是从 iCKB 的真实协议边界
反推 CellScript 必须具备什么能力：

1. 如果原始 iCKB 在某个 malformed 场景拒绝，CellScript 必须能在等价归一化场景中
   fail-closed。
2. 如果原始 iCKB 在某个边界值通过，CellScript 不能只用粗略 predicate 通过，而要把
   同一个 capacity、DAO rate、header dep、witness、script args 关系绑定清楚。
3. 如果某个 CellScript helper 只能覆盖一半语义，就只能登记为 supporting evidence，
   不能进入 selected differential rows。
4. 如果某个语义只能在 JSON model 中表达，就要从 active matrix 移除或标记 blocker，
   不能和真实执行证据混算。
5. 如果为了补一个 iCKB 场景需要协议特定 hack，就不能进入通用 `src/`，只能留在
   benchmark 层，直到抽象出协议中立能力。

这种方法的价值是：它把成熟度定义为“真实 CKB VM 里的可执行行为 + 可审计证据”，而
不是“语言看起来能表达”或“测试模型能跑通”。

## 希望 iCKB 作者重点审阅的内容

如果请 iCKB 作者 review，最有价值的反馈不是看 CellScript 语法是否漂亮，而是确认
这些语义映射是否忠实：

- 选定场景是否覆盖了 iCKB 的关键安全边界；
- normalized fixture 是否保留了 iCKB 原始语义，尤其是 DAO header、receipt data、
  xUDT owner-mode args、capacity compensation、Limit Order master relation 和
  Owned-Owner pairing；
- 原始 iCKB side 的 patched DAO hash deposit 测试是否作为测试环境功能性证据记录得
  足够清楚；
- pass/fail differential 是否是合适的第一阶段标准，哪些场景必须进一步要求数值或
  数据级完全一致；
- 哪些当前 blocker 对真实 iCKB 安全性最关键，应优先进入 P0；
- 哪些 benchmark fixture 过度简化，可能掩盖了 iCKB 原脚本的真实约束。

理想 review 结果不是“同意 CellScript 已经等价”，而是帮助确认下一批差分行应该补
在哪里，以及哪些 CellScript 通用能力必须先实现。

## 当前可复现实验入口

主要入口：

```bash
cargo test --locked -p cellscript --test ickb_diff -- --test-threads=1
cargo test --locked -p cellscript --test ickb_benchmark
cargo test --locked -p cellscript --test v0_17
cargo run --locked -p cellscript --bin cellc -- verify-ckb-fixtures tests/compat/ckb_standard/manifest.json --json
```

主要证据文件：

- `tests/benchmarks/ickb_diff/matrix.json`
- `tests/ickb_diff.rs`
- `tests/benchmarks/ickb_specs/ickb_logic.cell`
- `tests/benchmarks/ickb_specs/limit_order.cell`
- `tests/benchmarks/ickb_specs/owned_owner.cell`
- `docs/0.17/ickb_diff_results.md`
- `docs/0.17/ickb_cellscript_gap_matrix.md`
- `docs/0.17/ickb_production_equivalence_gate.md`
- `docs/0.17/ickb_progress.md`

## 给 iCKB 作者的一句话版本

这个分支把 iCKB 当成 CellScript 的真实 CKB 协议考试：每个能力都要从模型、编译、
CKB VM 执行、原始脚本差分、证据清单一路走完；当前 selected executed matrix 已经
通过 `PROVEN` 门禁，但 owner-auth、DAO redeem accounting、通用聚合降低、receipt
字节解码和更完整 production manifest 仍需要按同一标准继续扩展，不能把 selected
matrix 的结论说成全状态空间证明。
