# iCKB VM 差分进度文档

本文档用于跟踪 CellScript 0.17 的 iCKB 等价性工作进度。当前目标不是大范围重构，
而是继续补强真实 CKB VM 差分证据：原始 iCKB ELF 与生成的 CellScript ELF
必须在同一个归一化场景夹具上执行，记录通过/失败结果、哈希、周期、交易大小、
容量和失败模式。

## 仓库上下文

- 仓库：`/Users/arthur/RustroverProjects/CellScript`
- 分支：`research/protocol-equivalence`
- 上一个本地基线提交：`a0842ad Advance CellScript 0.17 iCKB benchmark gates`
- 最近 witness 基线提交：`a44431b docs: sync witness milestone progress`
- 本次工作范围：Witness/Molecule/Auth 解析闭环、0.18 Script API scope-control、DAO withdrawal capacity-compensation runtime formula evidence、DAO witness input_type 差分拒绝证据
- 忽略未跟踪目录：`typescript`
- 此前未跟踪路径已在 `9312bb2` 中提交并跟踪：
  - `tests/support/ckb_script_runner.rs`（已跟踪）
  - `tests/benchmarks/ickb_diff/original_binaries/`（已跟踪）
  - `tests/benchmarks/ickb_diff/ckb_vm_fixtures/`（已跟踪）
  - `tests/benchmarks/ickb_negative/` 下新增的 iCKB 负向夹具（已跟踪）

## 硬约束

- iCKB 专用逻辑必须留在：
  - `tests/benchmarks/`
  - `tests/support/`
  - `docs/0.17/`
- `src/` 只能加入协议中立的 CKB/CellScript 能力。
- 不要把 iCKB 专用运行时或编译器辅助函数加到 `src/`。
- 不要把大型 iCKB 仓库直接内嵌进本仓库。
- 不要削弱现有测试或生产等价性门禁。
- 不要把模型级夹具覆盖说成生产等价。
- 不要提交或暂存 `typescript`。

## 当前证据状态

当前选定矩阵已经通过生产等价门禁：

- 文件：`tests/benchmarks/ickb_diff/matrix.json`
- `mode = EXECUTED_CKB_VM_DIFF`
- `equivalence_status = PROVEN`
- `production_equivalence_claim = true`
- 当前行数：
  - 76 行：`DIFFERENTIAL_CKB_VM_EXECUTED`
  - 0 行：`MODEL`
- 支撑证据：
  - 14 行：`CELL_SCRIPT_CKB_VM_EXECUTED`
  - 8 行：`ORIGINAL_ICKB_CKB_VM_EXECUTED`

本轮清理后，已经有严格差分伴随行的 legacy model 行不再留在 active matrix
里重复计数。当前 active matrix 已没有 `MODEL` 行，矩阵顶层
`remaining_model_blockers` 为空，active `non_executable_model_assumptions`
也为空。3 个 legacy model 假设现在保留在 `retired_model_assumptions`：
`duplicate receipt` 的 receipt-id double-mint、
`wrong owner` 的 synthetic `owner` / `claimed_owner` 字段比较，以及
`immature redeem` 的 synthetic `current_epoch` / `maturity_epoch` 字段比较。
它们都不再作为 active row 计数；每一项都必须说明为什么当前可执行 iCKB
夹具没有这个字段形状，并绑定已有 replacement differential evidence。
同时 production gate 已加固：只要 active `non_executable_model_assumptions`
重新出现，任何 `PROVEN` / `EXECUTED_CKB_VM_DIFF` 声明都会被拒绝。

最近已验证的差分行：

说明：以下反引号中的内容是 `matrix.json` 的场景键，为了和测试、证据对象精确匹配，
保留原文。

- `differential: non-empty script args original vs CellScript agree`
- `differential: deposit phase 1 original vs CellScript agree`
- `differential: deposit too small original vs CellScript agree`
- `differential: deposit too big original vs CellScript agree`
- `differential: receipt without deposit original vs CellScript agree`
- `differential: duplicate receipt output original vs CellScript agree`
- `differential: receipt group exact mint original vs CellScript agree`
- `differential: receipt group over-mint original vs CellScript agree`
- `differential: receipt group missing header original vs CellScript agree`
- `differential: receipt group wrong accumulated rate original vs CellScript agree`
- `differential: receipt group wrong xUDT args original vs CellScript agree`
- `differential: receipt group malformed receipt data original vs CellScript agree`
- `differential: receipt group second malformed receipt data original vs CellScript agree`
- `differential: receipt group under-mint original vs CellScript agree`
- `differential: mint from receipt original vs CellScript agree`
- `differential: mint from malformed receipt data original vs CellScript agree`
- `differential: amount inflation original vs CellScript agree`
- `differential: amount deflation original vs CellScript agree`
- `differential: wrong xUDT args original vs CellScript agree`
- `differential: wrong accumulated rate original vs CellScript agree`
- `differential: missing header dep original vs CellScript agree`
- `differential: DAO mature withdrawal original vs CellScript agree`
- `differential: DAO immature withdrawal original vs CellScript agree`
- `differential: DAO max withdrawal capacity original vs CellScript agree`
- `differential: DAO deposit-rate adjusted max withdrawal capacity original vs CellScript agree`
- `differential: DAO deposit-rate adjusted over-withdraw capacity original vs CellScript agree`
- `differential: DAO withdraw-rate adjusted max withdrawal capacity original vs CellScript agree`
- `differential: DAO withdraw-rate adjusted over-withdraw capacity original vs CellScript agree`
- `differential: DAO wrong deposit accumulated rate original vs CellScript agree`
- `differential: DAO wrong withdraw accumulated rate original vs CellScript agree`
- `differential: DAO over-withdraw capacity original vs CellScript agree`
- `differential: DAO missing withdraw header original vs CellScript agree`
- `differential: DAO missing deposit header original vs CellScript agree`
- `differential: DAO deposit header index out of bounds original vs CellScript agree`
- `differential: DAO withdrawal deposit-data input original vs CellScript agree`
- `differential: DAO withdrawal malformed input data original vs CellScript agree`
- `differential: DAO missing witness input_type original vs CellScript agree`
- `differential: DAO empty witness input_type original vs CellScript agree`
- `differential: DAO short witness input_type original vs CellScript agree`
- `differential: DAO long witness input_type original vs CellScript agree`
- `differential: DAO wrong deposit header index original vs CellScript agree`
- `differential: DAO wrong withdraw committed header original vs CellScript agree`
- `differential: valid limit order original vs CellScript agree`
- `differential: limit order min-match boundary original vs CellScript agree`
- `differential: limit order underpayment original vs CellScript agree`
- `differential: limit order wrong asset original vs CellScript agree`
- `differential: limit order insufficient match original vs CellScript agree`
- `differential: limit order no CKB paid original vs CellScript agree`
- `differential: limit order UDT decreased original vs CellScript agree`
- `differential: valid limit order UDT-to-CKB original vs CellScript agree`
- `differential: limit order UDT-to-CKB min-match boundary original vs CellScript agree`
- `differential: limit order UDT-to-CKB no UDT paid original vs CellScript agree`
- `differential: limit order UDT-to-CKB wrong asset original vs CellScript agree`
- `differential: limit order UDT-to-CKB insufficient match original vs CellScript agree`
- `differential: limit order UDT-to-CKB underpayment original vs CellScript agree`
- `differential: valid owned-owner original vs CellScript agree`
- `differential: valid owned-owner output pairing original vs CellScript agree`
- `differential: owned-owner output relative mismatch original vs CellScript agree`
- `differential: owned-owner output duplicate owner original vs CellScript agree`
- `differential: owned-owner output missing owner original vs CellScript agree`
- `differential: owned-owner output missing owned original vs CellScript agree`
- `differential: owned-owner output script misuse original vs CellScript agree`
- `differential: owned-owner output non-withdrawal request original vs CellScript agree`
- `differential: owned-owner output owner data length mismatch original vs CellScript agree`
- `differential: owned-owner output related type hash mismatch original vs CellScript agree`
- `differential: owned-owner output related data rule mismatch original vs CellScript agree`
- `differential: owned-owner related type hash mismatch original vs CellScript agree`
- `differential: owned-owner related data rule mismatch original vs CellScript agree`
- `differential: owned-owner owner data length mismatch original vs CellScript agree`
- `differential: owned-owner relative mismatch original vs CellScript agree`
- `differential: owned-owner script misuse original vs CellScript agree`
- `differential: owned-owner non-withdrawal request original vs CellScript agree`
- `differential: owned-owner missing owner original vs CellScript agree`
- `differential: owned-owner missing owned original vs CellScript agree`
- `differential: owned-owner duplicate owner original vs CellScript agree`

此前新增 deposit 上界差分行：
`differential: deposit too big original vs CellScript agree`。该行使用
150,000,000,000,000 shannon 的 DAO deposit/receipt 输出和
400,000,000,000,000 shannon 的资金输入；已修补的原始 iCKB Logic 以
exit `8` 拒绝，CellScript 上界探针以 exit `7` 拒绝，失败模式
记录为 `deposit_capacity_upper_bound_rejected`。这补齐了 deposit phase 1
容量下界之外的上界拒绝证据，但仍不是 receipt/deposit 聚合降低。

本轮继续新增 receipt 组精确 mint、over-mint、missing-header、wrong-rate、
wrong-xUDT、第一条 malformed-receipt-data 和第二条 malformed-receipt-data 差分行，并保留此前
under-mint 拒绝行：
`differential: receipt group exact mint original vs CellScript agree`、
`differential: receipt group over-mint original vs CellScript agree`、
`differential: receipt group missing header original vs CellScript agree`、
`differential: receipt group wrong accumulated rate original vs CellScript agree`、
`differential: receipt group wrong xUDT args original vs CellScript agree`、
`differential: receipt group malformed receipt data original vs CellScript agree`、
`differential: receipt group second malformed receipt data original vs CellScript agree` 和
`differential: receipt group under-mint original vs CellScript agree`。八行都使用
两个同形状 receipt 输入，并把两个输入都链接到同一个 DAO header。精确 mint
行输出两份 receipt 对应的 xUDT，原始 iCKB Logic 通过并消耗 `96832` 周期，
CellScript 聚合探针通过并消耗 `26288` 周期。over-mint 行输出比两份
receipt 多 1 shannon 的 xUDT，under-mint 行只输出一份 receipt 对应的 xUDT；两条
坏行中原始 iCKB Logic 都以 exit `11` 拒绝，CellScript 聚合探针都以
exit `36` 拒绝，失败模式分别为 `receipt_group_over_mint` 和
`receipt_group_under_mint`。missing-header 行保持两份 xUDT 精确输出，但交易省略
DAO header dep，原始 iCKB Logic 以 exit `2` 拒绝，CellScript 聚合探针以
exit `28` 拒绝，失败模式为 `receipt_group_missing_header_dep`。wrong-rate 行
保留 header dep 和两份 xUDT 精确输出，但把两个 receipt 输入的 DAO accumulated
rate 改成 `20000000000000000`；原始 iCKB Logic 以 exit `11` 拒绝，CellScript
聚合探针以 exit `31` 拒绝，失败模式为
`receipt_group_wrong_accumulated_rate`。wrong-xUDT 行保留正确 header dep、正确
receipt rate 和两份 xUDT 精确输出，但把 xUDT owner-mode args 绑定到固定错误 owner
hash；原始 iCKB Logic 以 exit `11` 拒绝，CellScript 聚合探针以 exit `30`
拒绝，失败模式为 `receipt_group_wrong_xudt_binding`。这开始覆盖多 receipt
组金额聚合的精确/超额/不足、header-dep、DAO-rate 以及 xUDT
owner-mode args 双侧证据。malformed-receipt-data 行保留第二个 receipt、DAO
header、xUDT owner-mode args 和两份 xUDT 精确输出，但把第一个 receipt 输入 data
缩短到 4 字节；原始 iCKB Logic 以 exit `4` 拒绝，CellScript receipt-data-size
探针以 exit `37` 拒绝，失败模式为
`receipt_group_malformed_receipt_data`。新增的第二条 malformed 行保留第一个
receipt、DAO header、xUDT owner-mode args 和两份 xUDT 精确输出，但把第二个
receipt 输入 data 缩短到 4 字节；原始 iCKB Logic 以 exit `4` 拒绝，CellScript
receipt-data-size 探针以 exit `37` 拒绝，失败模式为
`receipt_group_second_malformed_receipt_data`。这把 receipt 字节形状的真实 VM
边界从第一个输入扩展到第二个输入，但仍不是完整 receipt 字节解码或
receipt-id double-mint 证明。

新增的 mint-family 差分行使用输入 receipt、原始 xUDT 二进制（`Data1` hash type）、
owner-mode args，以及 header-linked 输入 accumulated rate。通过、amount-inflation、
amount-deflation、wrong-rate 以及 missing-header-dep 行绑定到 script-under-test
hash；wrong-xUDT 行使用固定错误 owner hash。missing-header-dep 行把 receipt 输入
链接到 DAO header，但不把该 header 放入交易 header deps，因此双方都会拒绝同一个夹具。
本轮再新增 `differential: mint from malformed receipt data original vs CellScript agree`：
该单 receipt 夹具保留 DAO header、xUDT owner-mode args 和精确 xUDT 输出，
但把 receipt 输入 data 缩短到 4 字节；原始 iCKB Logic 以 exit `4` 拒绝，
CellScript receipt-data-size 探针以 exit `37` 拒绝，失败模式为
`mint_malformed_receipt_data`。CellScript 侧目前检查输入 rate、xUDT owner args、
输出 xUDT amount，并在新增 malformed-data 行检查 receipt data size；完整 receipt
字节解码与多 receipt/deposit 聚合重算仍未完成，因此仍不能声明
iCKB 等价。

本轮新增两个 CellScript-only CKB VM 前置证据：
`ckb vm harness fail: DAO immature redeem relative since` 和
`ckb vm harness pass: DAO mature redeem relative since`。两条夹具都使用
withdrawal request 输入；失败行把相对 epoch since 设置为 `359/0/1`，CellScript
脚本要求 `360/0/1`，在真实 CKB VM 中通过 `dao::require_input_since_at_least`
和 `dao::require_input_relative_epoch_since_at_least` 以
`DaoMaturityViolation`/exit `36` 拒绝；通过行把 since 设置为 `360/0/1`
并通过同一个脚本。它们推进了 redeem maturity 的可执行通过/拒绝边界，
但没有运行原始 iCKB/DAO 侧，所以不是差分等价行。

本次累计新增七条协议中立的 CellScript-only CKB VM witness 回归测试，不新增
`matrix.json` 行：`cellscript_witness_args_empty_lock_passes_in_ckb_vm` 在真实
CKB VM 中执行 `witness::size`、`witness::raw` 和 `witness::lock`，确认 16 字节空
WitnessArgs（total_size + 3 offsets）可通过，`BytesOpt::None` 的 lock 字段被解析为
零填充 Hash；`cellscript_require_witness_size_at_least_rejects_too_small_in_ckb_vm`
确认 `ckb::require_witness_size_at_least` 在 `min_size > actual_size` 时以
`WitnessMalformed(42)` fail-closed；`cellscript_witness_args_short_lock_is_zero_padded_in_ckb_vm`
确认短 `BytesOpt::Some([0])` 字段不会带出未初始化栈字节，而是零填充为 32 字节
Hash；`cellscript_witness_args_lock_input_type_output_type_are_isolated_in_ckb_vm`
确认 `lock`、`input_type`、`output_type` 三个字段会读成互不串位的非零 Hash；
该正向字段隔离测试使用 `ckb-types` 的 `packed::WitnessArgs` builder 产物；
`cellscript_witness_args_total_size_mismatch_rejects_in_ckb_vm`、
`cellscript_witness_args_reordered_offsets_reject_in_ckb_vm` 和
`cellscript_witness_args_truncated_offsets_reject_in_ckb_vm` 分别覆盖 total_size
不匹配、offset 乱序和 offset 越界的 fail-closed 路径。该组测试实际走
`LOAD_WITNESS` 和 `ckb-testtool` 交易上下文，比单纯
assembly 生成检查更强，但仍不是原始 iCKB vs CellScript 双侧差分行。

本轮继续新增三条原始 DAO 二进制单侧 CKB VM 行：
`original DAO binary: creates withdrawing cell`、
`original DAO binary: mature withdrawal passes` 和
`original DAO binary: immature withdrawal rejects`。它们执行未修改的 DAO ELF：
phase-1 deposit-to-withdrawing-cell 创建路径通过；phase-2 withdrawal 使用 deposit
header、withdraw header 和 witness `input_type = 1`，在 mature since
`0x2003e8022a0002f3` 下通过；同形状夹具把 since 降为
`0x2003e802290002f3` 时以 `ERROR_INCORRECT_SINCE (-17)` 拒绝。这强化了
DAO/header lineage 和 maturity 的原始脚本侧证据，但还不是原始 DAO vs CellScript
的双侧差分行。

本轮再把 phase-2 DAO withdrawal maturity/header/data-shape/capacity/rate/witness 升级成二十一条真实差分行：
`differential: DAO mature withdrawal original vs CellScript agree`、
`differential: DAO immature withdrawal original vs CellScript agree`、
`differential: DAO max withdrawal capacity original vs CellScript agree`、
`differential: DAO deposit-rate adjusted max withdrawal capacity original vs CellScript agree`、
`differential: DAO deposit-rate adjusted over-withdraw capacity original vs CellScript agree`、
`differential: DAO withdraw-rate adjusted max withdrawal capacity original vs CellScript agree`、
`differential: DAO withdraw-rate adjusted over-withdraw capacity original vs CellScript agree`、
`differential: DAO wrong deposit accumulated rate original vs CellScript agree`、
`differential: DAO wrong withdraw accumulated rate original vs CellScript agree`、
`differential: DAO over-withdraw capacity original vs CellScript agree`、
`differential: DAO missing withdraw header original vs CellScript agree`、
`differential: DAO missing deposit header original vs CellScript agree`、
`differential: DAO deposit header index out of bounds original vs CellScript agree`、
`differential: DAO withdrawal deposit-data input original vs CellScript agree`、
`differential: DAO withdrawal malformed input data original vs CellScript agree`、
`differential: DAO missing witness input_type original vs CellScript agree`、
`differential: DAO empty witness input_type original vs CellScript agree`、
`differential: DAO short witness input_type original vs CellScript agree`、
`differential: DAO long witness input_type original vs CellScript agree`、
`differential: DAO wrong deposit header index original vs CellScript agree` 和
`differential: DAO wrong withdraw committed header original vs CellScript agree`。mature 与
immature 两行使用同一个归一化夹具形状：一个 withdrawing DAO input、
withdraw/deposit 两个 header dep、witness `input_type = 1` 和一个 withdrawn
capacity output。mature since `0x2003e8022a0002f3` 双侧通过；immature since
`0x2003e802290002f3` 双侧拒绝，原始 DAO exit `-17`，CellScript exit `36`。
max-capacity 行保持 mature since、withdraw/deposit header、witness
`input_type = 1` 和 withdrawal data 形状不变，并用 CellScript 容量探针在运行时读取
`cell_capacity`、`cell_occupied_capacity`、`dao::input_accumulated_rate` 和
deposit header `dao::accumulated_rate`，计算
`occupied + ((input_capacity - occupied) * withdraw_rate / deposit_rate)`，确认
原始 DAO 容量边界 `123468305678` shannon 双侧通过。over-withdraw capacity
行使用同一形状，但把 withdrawal output capacity 设置为 `123468305679` shannon，
比该实测边界多 1。原始 DAO 以 exit `-15` 拒绝，CellScript runtime formula
探针以 exit `48` 拒绝，失败模式为 `dao_over_withdraw_capacity`。
deposit-rate-adjusted-max 行把 deposit header accumulated_rate 改为 `10000001`，
同时把 withdrawal output capacity 设置为 fixture-rate 最大值 `123468294151`
shannon；原始 DAO 与 CellScript runtime formula 探针均通过。withdraw-rate-adjusted-max
行把 withdraw header accumulated_rate 改为 `10000999`，同时把 output capacity
设置为 fixture-rate 最大值 `123468294152` shannon；双侧同样通过。这两条正向行证明
容量公式确实读取并使用 fixture header rate，而不是只在错误场景中拒绝。
本轮继续新增 deposit-rate-adjusted-over 和 withdraw-rate-adjusted-over 两条
`fixture_rate_plus_one` 拒绝行：前者把 output capacity 设置为 `123468294152`，
比 deposit fixture-rate 最大值多 1 shannon；后者设置为 `123468294153`，
比 withdraw fixture-rate 最大值多 1 shannon。两条都由原始 DAO 以 exit `-15`
拒绝，CellScript runtime formula 探针以 exit `48` 拒绝，证明 adjusted-rate
边界不是近似检查，而是精确上界。
wrong-deposit-rate 行保持输出容量在 `123468305678`，但把 deposit header
accumulated_rate 从 `10000000` 改成 `10000001`；CellScript 侧不再硬编码正确
rate，而是用 fixture rate 计算出最大容量 `123468294151`，确认该输出多取
`11527` shannon。原始 DAO 以 exit `-15` 拒绝，CellScript runtime formula 探针
以 exit `48` 拒绝，失败模式为 `dao_wrong_deposit_accumulated_rate`。
wrong-withdraw-rate 行保持输出容量在 `123468305678`，但把 withdraw header
accumulated_rate 从 `10001000` 改成 `10000999`；同一个 runtime formula 计算出
fixture 最大容量 `123468294152`，确认该输出多取 `11526` shannon。原始 DAO 以
exit `-15` 拒绝，CellScript runtime formula 探针以 exit `48` 拒绝，失败模式为
`dao_wrong_withdraw_accumulated_rate`。这八条行现在覆盖选定 DAO withdrawal
capacity compensation 公式的正确 rate 最大值、fixture-rate 最大值、fixture-rate
plus-one 拒绝、correct-rate overdraw 拒绝以及 deposit/withdraw rate 变动拒绝；
矩阵 normalized fixture 记录 occupied、withdrawable、
withdraw/deposit rates、correct-rate max 和 fixture-rate max。但这仍不是通用多输入
redeem accounting 或 `assert_delta` lowering。
missing-withdraw-header 行保持 mature since、deposit header 和同一个 output
capacity，但省略 input committed header 所需的 withdraw header dep，并把 witness
`input_type` 指向现有 deposit header index `0`；原始 DAO exit `2`，CellScript
input-header 探针 exit `28`，失败模式为 `dao_missing_withdraw_header`。
missing-deposit-header 行保持 mature since、withdraw header 和同一个 output
capacity，但省略 witness `input_type = 1` 指向的 deposit header dep；原始 DAO
exit `1`，CellScript deposit-header 探针 exit `28`，失败模式为
`dao_missing_deposit_header`。
deposit-header-index-out-of-bounds 行保持 withdraw/deposit 两个 header dep 都存在，
但 witness `input_type` 指向越界的 header dep index `2`；原始 DAO exit `1`，
CellScript out-of-bounds header 探针 exit `28`，失败模式为
`dao_deposit_header_index_out_of_bounds`。
wrong-deposit-header-index 行保持 withdraw/deposit 两个 header dep 都存在，但把
witness `input_type` 从 deposit header index `1` 改成 withdraw header index `0`；
原始 DAO exit `-14`，CellScript deposit-header witness 探针 exit `41`，失败
模式为 `dao_wrong_deposit_header_index`。wrong-withdraw-committed-header 行保留
withdraw/deposit 两个 header dep 和 witness `input_type = 1`，但把 withdrawing
input 的 committed header 错绑成 deposit header；原始 DAO exit `-14`，
CellScript input-header 探针 exit `40`，失败模式为
`dao_wrong_withdraw_committed_header`。deposit-data input 行保留 mature since、
withdraw/deposit 两个 header dep、witness `input_type = 1` 和同一个 output
capacity，但把 input data 从 withdrawal-request block number 改成
`0x0000000000000000`；原始 DAO exit `2`，CellScript withdrawal-data classifier
探针 exit `34`，失败模式为 `dao_withdrawal_deposit_data_input`。
malformed-input-data 行保留相同 mature/header/witness/output 形状，但把 input data
缩短成 `0x12060000`；原始 DAO exit `-4`，CellScript classifier 探针 exit `34`，
失败模式为 `dao_withdrawal_malformed_input_data`。missing-witness-input_type 行保持
mature since、withdraw/deposit 两个 header dep、withdrawal data 和 output capacity
有效，但 witness 完全省略 `input_type`；empty-witness-input_type 行则提供
`input_type` 字段但 payload 长度为 0；short-witness-input_type 行提供
`0x01` 这个非空 1 字节 payload；long-witness-input_type 行提供
`0x010000000000000099` 这个 9 字节 payload，而不是 DAO 期望的 8 字节
little-endian header dep index。四条都由原始 DAO 以 exit `-11` 拒绝；
CellScript WitnessArgs input_type 探针对 missing/empty 以 exit `42` 拒绝，
对短/长宽度以 exit `43` 拒绝，失败模式分别为
`dao_missing_witness_input_type`、`dao_empty_witness_input_type`、
`dao_short_witness_input_type` 和 `dao_long_witness_input_type`。本轮还把协议中立
`dao::accumulated_rate(source::header_dep(i))` 与
`dao::require_header_dep_for_input(input, header)` 从无效的 DAO
`LOAD_HEADER_BY_FIELD` 路径切到 `LOAD_HEADER` 绝对 offset 读取。

新增的 duplicate receipt output 差分行使用一个 DAO deposit output 和两个相同金额的
receipt output。原始 iCKB Logic 经 DAO hash 修补后在输出 type script 上以
`ReceiptMismatch`/exit `10` 拒绝，CellScript 探针也以 exit `10` 拒绝同一个
归一化夹具。这个行覆盖 deposit/receipt output accounting 的一个真实 VM 拒绝，
但它不是 model 里 receipt-id double-mint 的完整替代。

新增的 Limit Order 差分行使用原始 `limit_order` 二进制作为 lock script，
配合共享 auxiliary UDT type code cell。输入订单编码 `Action::Mint`，输出订单编码
`Action::Match` 并绑定同一个 master OutPoint；CellScript 侧执行夹具绑定的
CKB+UDT value conservation 检查，并在 CKB-to-UDT wrong-asset 行检查
type-hash low-word 一致性，在 insufficient-match 行检查 `ckb_min_match = 64`
的最小成交边界。
本轮新增 CKB-to-UDT 精确 min-match 边界通过行：输出订单 CKB 正好减少
`64` shannon，输出 UDT 正好增加 `64`，原始 Limit Order 消耗 `60247` 周期
通过，CellScript 探针消耗 `11199` 周期通过，手续费为 `64` shannon。这补上
`1 << min_match_log` 等号边界的双侧 VM 证据。
近期新增 no-CKB-paid 行，检查输出 CKB 没有下降时必须拒绝成交；也新增
UDT-decreased 行，检查输出订单 UDT 数量低于输入订单数量时必须拒绝。随后新增
UDT-to-CKB 有效行，覆盖反向成交分支：输入订单 UDT 被完全支付，输出订单 CKB
增加，并使用归一化资金输入补足容量。本轮再新增 UDT-to-CKB exact
min-match 边界通过行：输出订单 CKB 正好增加 `64` shannon，输出 UDT
正好减少 `64`，原始 Limit Order 消耗 `65395` 周期通过，CellScript 探针
消耗 `13530` 周期通过，手续费为 `9999999936` shannon。近期继续新增
UDT-to-CKB no-UDT-paid 拒绝行，覆盖输出订单保留完整 UDT 且没有收到 CKB 支付的反向坏成交。
本轮继续新增 UDT-to-CKB wrong-asset 拒绝行，覆盖反向成交输出 UDT type
script hash 不一致的资产绑定失败。随后新增 UDT-to-CKB insufficient-match 行，
覆盖订单价值守恒但 UDT delta 只有 50、小于 `ckb_min_match = 64` 的反向部分成交拒绝。
本轮继续新增 UDT-to-CKB underpayment 拒绝行，覆盖完整 10,000,000,000 UDT fill
只换入 5,000,000,000 CKB 的反向价值短缺拒绝。
它们提升了选定 Limit Order 路径的真实 VM 证据，但还不是完整
一等 MetaPoint/OutPoint map 或完整 `Script` equality 语义。

新增的 Owned-Owner 差分行使用原始 `owned_owner` 二进制和生成的 CellScript ELF，
在同一个 lock-owned/type-owner 输入夹具形状上执行。valid 行把 owner cell 的
i32 relative distance 写成 `1`，从 owner OutPoint index 1 指向 owned withdrawal
request index 2，双方都通过；relative-mismatch 行写成 `-1`，指向 index 0 而不是
owned cell，双方都拒绝。原始 `owned_owner` 二进制的硬编码 DAO hash 被修补到
共享 auxiliary withdrawal type hash，以适配 ckb-testtool；这是有记录的功能性
测试桥接，不是主网身份重建。这两行覆盖具体 MetaPoint pair 通过/拒绝，
还不等于完整 wrong-owner 资源语义。

本轮新增的 Owned-Owner script-misuse 行使用单输入夹具：同一个 cell 同时把脚本
作为 lock 和 type。原始 `owned_owner` 二进制未做 DAO hash 修补，因为它在
DAO type/data 分类之前就走 `ScriptMisuse` 分支并以 exit `7` 拒绝；CellScript
探针先确认 lock/type hash 都等于当前脚本，再以 exit `7` 拒绝。这覆盖了
Owned-Owner 的脚本角色误用路径，但仍不等于模型级 `wrong owner` 资源字段语义。
本轮继续新增 non-withdrawal request 行：输入 0 只把脚本作为 lock，没有 DAO
withdrawal type/data；原始二进制未修补，并以 `NotWithdrawalRequest`/exit `6`
拒绝，CellScript 探针也以 exit `6` 拒绝同一个归一化夹具。这个行覆盖
Owned-Owner lock-owned cell 必须是 DAO withdrawal request 的原始分支，但仍不是
完整 wrong-owner 资源语义。
本轮继续新增 missing-owner 行：输入 0 是已修补 DAO hash 下可识别的
withdrawal request owned cell，但没有对应的 type-owner cell；原始 `owned_owner`
以 `Mismatch`/exit `8` 拒绝，CellScript pairing 辅助函数以 exit `40` 拒绝。
这条行第一次把 Owned-Owner 的“owned 和 owner 必须一一配对”缺失侧也落到真实 VM
差分证据里，但仍不是完整 BTreeMap/MetaPoint API。
本轮继续新增 missing-owned 行：输入 0 是 type-owner cell，data 里的 relative
distance 指向不存在的 owned cell；原始 `owned_owner` 未做 DAO hash 修补，并在 type
script 上以 `Mismatch`/exit `8` 拒绝，CellScript 辅助函数在 type script 上以
exit `40` 拒绝。它补齐了 pair accounting 的另一侧缺失证据，但仍不是完整
一等 MetaPoint map。
本轮继续新增 duplicate-owner 行：同一个 owned withdrawal request 有两个
type-owner cell 指向它；原始 `owned_owner` 经 DAO hash 修补后以 `Mismatch`/exit
`8` 拒绝，CellScript 辅助函数以 exit `40` 拒绝。它覆盖了 owner 数量大于 1
的基数分支，但仍不等于完整 wrong-owner 资源语义。
本轮继续新增 output-side valid pairing 行：脚本作为 output type 执行，输出 0 是
lock-owned withdrawal request，输出 1 是 type-owner cell，owner cell data 写入
`-1` 并指向 output 0；已修补的原始 `owned_owner` 以 47,359 周期通过，
CellScript output-source 辅助函数以 20,069 周期通过。它补上了 Owned-Owner
Source::Output MetaPoint pairing 的通过证据，但仍不是完整一等
MetaPoint map。
本轮继续新增 output-side relative-mismatch 行：同样在 output type 上执行，但
owner output 的 i32 distance 写成 `1`，指向不存在的 output 2，而不是 output 0。
已修补的原始 `owned_owner` 以 `Mismatch`/exit `8` 拒绝，CellScript output-source
辅助函数以 exit `40` 拒绝。它让 Owned-Owner Source::Output pairing 具备通过/拒绝
双侧证据，但仍不覆盖完整 wrong-owner 资源语义。
本轮继续新增 output-side duplicate-owner 行：output 0 是 lock-owned withdrawal
request，outputs 1/2 是 type-owner cells，分别用 `-1` 和 `-2` 指回 output 0。
已修补的原始 `owned_owner` 以 `Mismatch`/exit `8` 拒绝，CellScript output-source
辅助函数以 exit `40` 拒绝，失败模式为 `output_duplicate_owner_pair`。它覆盖
Source::Output 下 owner 数量大于 1 的基数分支，但仍不等于完整 wrong-owner
资源语义。
本轮继续新增 output-side missing-owner 与 missing-owned 两行。missing-owned 行只有
一个 output type-owner cell，distance `1` 指向不存在的 lock-owned output；原始
`owned_owner` 未修补，并以 `Mismatch`/exit `8` 拒绝，CellScript 辅助函数以
exit `40` 拒绝。missing-owner 行用 output 2 的 type-owner 指向 output 1，同时
output 0 也是 lock-owned withdrawal request 但没有 owner；已修补的原始
`owned_owner` 以 exit `8` 拒绝，CellScript output-source 辅助函数以 exit `40`
拒绝。它们补齐 Source::Output 下缺失配对的双侧 VM 证据，但仍不是完整
一等 MetaPoint map。
本轮继续新增 output-side script-misuse 行：output 0 同时把脚本作为 lock 和
type，脚本通过 output type 执行。原始 `owned_owner` 未修补，并在 DAO
type/data 分类之前以 `ScriptMisuse`/exit `7` 拒绝；CellScript output misuse
探针也以 exit `7` 拒绝同一个归一化夹具。它补齐了脚本角色误用在
Source::Output 下的拒绝证据，但仍不是完整 owned 资源语义。
本轮继续新增 output-side non-withdrawal request 行：output 1 作为 type-owner
触发脚本执行，output 0 使用脚本作为 lock，但没有 DAO withdrawal type/data。
原始 `owned_owner` 未修补，并以 `NotWithdrawalRequest`/exit `6` 拒绝；
CellScript output non-withdrawal 探针也以 exit `6` 拒绝同一个归一化夹具。
它覆盖 Source::Output 下 lock-owned withdrawal-shape guard，但仍不是完整
Owned-Owner 资源 owner 语义。
本轮继续新增 output-side owner data length mismatch 行：output 0 是 lock-owned
withdrawal request，output 1 是 type-owner cell，但 owner output data 只有 3 字节，
不能解码成 4 字节 little-endian `i32` relative MetaPoint distance。已修补的原始
`owned_owner` 在 output type 上以 exit `4` 拒绝，CellScript output-source 辅助函数
以 `ScriptFieldMalformed`/exit `34` 拒绝。它把 owner-side distance data decoding
边界扩展到 Source::Output，但仍不是完整 wrong-owner 资源语义。
本轮继续新增 output-side related type hash mismatch 行：原始 `owned_owner` 被修补
到预期 auxiliary withdrawal type hash，但 lock-owned output 实际使用同一 auxiliary
code、非空 args 的 type script，因此 type hash 不匹配；output data 仍是合法
withdrawal request，owner output 的 relative distance 也合法指回 output 0。原始脚本以
`NotWithdrawalRequest`/exit `6` 拒绝，CellScript output 探针先检查预期 related type
hash low-word，再以 exit `46` 拒绝。它把 Source::Output related-cell type binding
也落到真实 VM 证据里，但 CellScript 侧仍是 low-word 夹具绑定探针，不是完整
一等 `Script` equality 或模型级 `wrong owner` 资源字段语义。
本轮继续新增 output-side related data rule mismatch 行：原始 `owned_owner` 同样被修补
到预期 auxiliary withdrawal type hash，lock-owned output 的 related type hash 匹配，
但 data 改成 8 字节 zero/deposit marker，而不是非零 withdrawal request payload；
owner output 的 relative distance 仍合法指回 output 0。原始脚本以
`NotWithdrawalRequest`/exit `6` 拒绝，CellScript output 探针在通过 related type
hash low-word 检查后调用 withdrawal data guard，并以 exit `47` 拒绝。它把
Source::Output related-cell data-rule mismatch 也落到双侧 VM 证据里，但仍不是完整
wrong-owner 资源语义或完整 `Script` equality。
本轮继续新增 input-side related type hash mismatch 行：原始 `owned_owner` 被修补到
预期 auxiliary withdrawal type hash，但 lock-owned input 实际使用同一 auxiliary code、
非空 args 的 type script，因此 type hash 不匹配；该 cell 仍带有非零 withdrawal
request data，以隔离 type-hash mismatch。原始脚本以 `NotWithdrawalRequest`/exit `6`
拒绝，CellScript 探针先检查预期 related type hash low-word，再以 exit `46` 拒绝。
这条行推进了 Owned-Owner related-cell type binding 的真实 VM 证据，但 CellScript 侧
仍是 low-word 夹具绑定探针，不是完整一等 `Script` equality 或
模型级 `wrong owner` 资源字段语义。
本轮继续新增 input-side related data rule mismatch 行：原始 `owned_owner` 同样被
修补到预期 auxiliary withdrawal type hash，lock-owned input 的 related type hash
匹配，但 data 改成 8 字节 zero/deposit marker，而不是非零 withdrawal request
payload。原始脚本以 `NotWithdrawalRequest`/exit `6` 拒绝，CellScript 探针在通过
related type hash low-word 检查后调用 withdrawal data guard，并以 exit `47` 拒绝。
这条行把 related-cell data-rule mismatch 也落到双侧 VM 证据里，但仍不是完整
wrong-owner 资源语义或完整 `Script` equality。
本轮继续新增 owner data length mismatch 行：lock-owned input 是可识别的
withdrawal request，type-owner cell 也存在且使用脚本作为 type，但 owner data 只有
3 字节，不能解码成 4 字节 little-endian `i32` relative MetaPoint distance。
已修补的原始 `owned_owner` 以 exit `4` 拒绝，CellScript 辅助函数在读取距离字段时以
`ScriptFieldMalformed`/exit `34` 拒绝。它覆盖 owner-side distance data decoding
边界，但仍不是完整模型级 `wrong owner` 资源字段语义。

关键验证命令：

```bash
cargo test --locked -p cellscript --test ickb_diff -- --test-threads=1
```

结果：

```text
106 passed; 0 failed
```

说明：行级证据名称使用 `DIFFERENTIAL_CKB_VM_EXECUTED`；生产门禁模式仍使用
`EXECUTED_CKB_VM_DIFF` / `PROVEN`。不要引入新的同义名称。

## 里程碑进度表

这些百分比只用于规划，不代表全状态空间证明。这里把“选定夹具的生产等价门禁”
和“未选定场景的后续扩展”分开记录：active matrix 已经升级为
`EXECUTED_CKB_VM_DIFF` / `PROVEN`，且只包含 76 条双侧 CKB VM 差分行；真实
owner-auth witness、DAO redeem aggregate accounting、通用聚合降低和更完整
manifest closure 仍是后续扩展方向，不能被这 76 条 selected rows 自动外推覆盖。
一等公民 `Script` API 明确不属于 0.17 交付范围；它移动到下一个版本 0.18。
0.17 只保留 helper-level Script support，用于继续补 iCKB 等价证据。

| 里程碑 | 当前估计 | 当前状态 | 下一步 |
|---|---:|---|---|
| 协议中立 CKB/CellScript 基础能力 | 82-85% | DAO rate/data/type 辅助函数、HeaderDep DAO rate/lineage 辅助函数、xUDT 金额检查、SourceView 脚本身份（含 `code_hash + hash_type` 身份要求）、OutPoint、MetaPoint 扫描（含 filtered pair scan）、`u128`、C256、带符号 `i32` 已有较多基础；HeaderDep DAO 辅助函数已从 `LOAD_HEADER_BY_FIELD` 切到 `LOAD_HEADER` 绝对 offset；新增 `ScriptIdentityMismatch`(41) 运行时错误。 | 继续补齐任意 args 访问、通用 group 扫描。 |
| CKB VM 执行基础设施 | 99% | `ckb-testtool` 执行框架已可加载原始二进制、执行 CellScript ELF，并通过 76 条差分行；本轮新增 CellDep 数据读取回归行，证明 fixture `cell_deps` 会真正进入交易 CellDep 列表；另确认 duplicate receipt-id、wrong-owner synthetic resource fields、immature-redeem synthetic epoch fields 都是非 active row 的模型假设，并分别绑定 receipt group exact-mint、valid Owned-Owner、DAO immature withdrawal replacement evidence；另有 mature/immature redeem relative-since CellScript-only 通过/拒绝证据、三条原始 DAO 二进制通过/拒绝证据，以及二十一条 DAO phase-2 withdrawal maturity/header/data-shape/capacity/rate/witness 差分证据。 | 抽象更多可复用夹具，扩展到真实 owner-auth 字节、withdraw/redeem 聚合场景。 |
| 原始 iCKB 单侧 VM 证据 | 96-98% | 原始 iCKB Logic、Limit Order、Owned-Owner 与未修改 DAO ELF 已执行多个 VM 场景；已修补 DAO hash 的 deposit phase 1 通过，观测周期为 `97057`，并新增 deposit 上界 exit `8` 拒绝；原始 DAO phase-1 withdrawing cell 创建通过，phase-2 mature withdrawal 通过，phase-2 max capacity、deposit-rate adjusted max 和 withdraw-rate adjusted max 均通过，phase-2 immature since 以 exit `-17` 拒绝，phase-2 over-withdraw capacity、deposit-rate adjusted over-withdraw、withdraw-rate adjusted over-withdraw、wrong deposit accumulated_rate 和 wrong withdraw accumulated_rate 均以 exit `-15` 拒绝，phase-2 missing withdraw header 与 deposit-data input 均以 exit `2` 拒绝，phase-2 malformed input data 以 exit `-4` 拒绝，phase-2 missing deposit header 与 deposit header index 越界均以 exit `1` 拒绝，phase-2 wrong deposit header index 与 wrong withdraw committed header 均以 exit `-14` 拒绝，phase-2 missing/empty/short/long witness input_type 均以 exit `-11` 拒绝；valid Owned-Owner input pairing 观测周期为 `83458`，valid output pairing 观测周期为 `47359`；Owned-Owner input/output owner data length mismatch 均以 exit `4` 拒绝，input/output script misuse 均以 exit `7` 拒绝，input/output non-withdrawal request、input/output related type-hash mismatch 与 input/output related data-rule mismatch 均以 exit `6` 拒绝，input/output missing-owner、input/output missing-owned、input/output duplicate-owner pair、input/output relative mismatch 均以 exit `8` 拒绝。 | 继续补 DAO redeem aggregate accounting 与更多 committed header 替换边界。 |
| 双侧 CKB VM 差分证据 | PROVEN（选定夹具矩阵） | 已有 76 条真实差分行：13 条通过（deposit phase 1、receipt group exact mint、mint from receipt、DAO mature withdrawal、DAO max withdrawal capacity、DAO deposit-rate adjusted max withdrawal capacity、DAO withdraw-rate adjusted max withdrawal capacity、valid limit order CKB-to-UDT、limit order CKB-to-UDT min-match boundary、valid limit order UDT-to-CKB、limit order UDT-to-CKB min-match boundary、valid Owned-Owner input pairing、valid Owned-Owner output pairing）和 63 条拒绝；active `MODEL` 行已清零，选定矩阵已通过 `EXECUTED_CKB_VM_DIFF` / `PROVEN` 门禁。 | 继续补真实 owner-auth 字节夹具、withdraw/redeem 聚合 accounting 和 DAO redeem accounting 差分，作为未来新增矩阵行。 |
| DAO/header 血缘证明 | 96-98% | wrong accumulated rate 和 missing header dep 已进入单 receipt 差分证据；本轮新增 receipt group missing-header 拒绝和 wrong-rate 拒绝，证明两个 receipt 输入的 group 聚合也依赖交易 header dep 与 DAO accumulated_rate；另有 mature/immature redeem relative-since CellScript-only VM 通过/拒绝，三条 original DAO phase-1/phase-2 通过/拒绝，并新增二十一条 DAO phase-2 withdrawal maturity/header/data-shape/capacity/rate/witness 差分行，覆盖 deposit header、withdraw header、witness input_type、immature since 拒绝、max capacity 通过、deposit-rate adjusted max 通过、deposit-rate adjusted over-withdraw 拒绝、withdraw-rate adjusted max 通过、withdraw-rate adjusted over-withdraw 拒绝、over-withdraw capacity 拒绝、wrong deposit accumulated_rate 拒绝、wrong withdraw accumulated_rate 拒绝、missing withdraw header 拒绝、missing deposit header 拒绝、deposit header index 越界拒绝、deposit-data input 拒绝、malformed input-data 拒绝、missing witness input_type 拒绝、empty witness input_type 拒绝、short witness input_type 拒绝、long witness input_type 拒绝、wrong deposit header index 拒绝与 wrong committed withdraw header 拒绝；本轮把 max/adjusted-max/adjusted-over/over/wrong-deposit-rate/wrong-withdraw-rate 八条的 CellScript 侧推进为 runtime capacity compensation formula，夹具记录 occupied、withdrawable、withdraw/deposit rates、correct-rate max、fixture-rate max 和 fixture-rate plus-one 边界；但多输入 redeem accounting 仍未完整双侧差分执行。 | 优先把 redeem aggregate accounting、receipt pairing 和更多 DAO 边界配成双侧 VM 夹具。 |
| Witness/Molecule/Auth 解析 | 100%（协议中立解析闭环） | 已实现 `witness::size`（U64，标量 fail-closed 约定）、`witness::raw`（Hash，带 caller buffer 桥接）和 `ckb::require_witness_size_at_least`（void runtime requirement，正确保存 `min_size` 与栈帧管理）；已实现完整 WitnessArgs Molecule 表头解析（16 字节头：total_size + 3 offsets，field_count 从 offset0/4-1 推导）与 BytesOpt 字段提取（lock/input_type/output_type，带 caller buffer 写入、None 支持、短字段零填充）；Hash 返回路径已加入 `emit_runtime_witness_hash_call` 桥接函数（分配 caller buffer、参数 set-up、状态检查、存储 buffer 指针）；`allocate_var_storage` 已为 witness Hash 调用分配 32 字节缓冲区；新增 `WitnessMalformed(42)` 和 `WitnessFieldTruncated(43)` 运行时错误；七条真实 CKB VM witness 回归覆盖空 WitnessArgs、too-small fail-closed、短字段零填充、ckb-types builder 产物的 lock/input_type/output_type 字段隔离、total_size mismatch、offset 乱序和 offset 越界；类型推断、IR、LSP、feature flags、runtime access metadata 和 proof-plan `reads: witness` 已对齐。Auth 口径固定为显式数据源/claim 授权域检查，不引入隐式 signer 语义。 | 解析器项关闭；后续真实 owner-auth witness 字节属于生产等价夹具/证据，不再作为解析器缺口。 |
| 一等公民 `Script` API | 0.18 scope（0.17 不交付） | 0.17 只保留 helper-level Script support，已覆盖 active iCKB 差分矩阵需要的 `Script identity + args binding + role`：`require_cell_*_script_hash_type`、args empty/hash、xUDT owner-mode args、filtered pair scan 等；不在 0.17 做 arbitrary Script constructor 或一等 Script value。 | 0.18 再做只读 ScriptRef/ScriptArgs：`code_hash`、`hash_type`、`args_empty`、`args_hash`、exact/prefix/suffix args checks；禁止 deploy manifest resolution、TYPE_ID constructor、任意 script hash synthesis。 |
| 可执行聚合降低 | 72-75% | mint-family 差分已覆盖 xUDT mint amount、amount inflation/deflation、错误 xUDT args、错误 accumulated rate、缺失 header dep，并新增单 receipt malformed-data 拒绝、两个 receipt 输入精确 mint 两份 xUDT 的 group exact-mint 通过、比两份多 1 shannon 的 group over-mint 拒绝、只 mint 一份 xUDT 的 group under-mint 拒绝、group missing-header 拒绝、group wrong-rate 拒绝、group wrong-xUDT 拒绝、group first malformed-receipt-data 拒绝、group second malformed-receipt-data 拒绝和 group missing-second-input 拒绝；deposit phase 1 已覆盖下界和上界 capacity 拒绝，deposit/receipt output accounting 已新增 duplicate receipt output `ReceiptMismatch` 拒绝；DAO withdrawal max、deposit-rate adjusted max、deposit-rate adjusted over、withdraw-rate adjusted max、withdraw-rate adjusted over、over、wrong-deposit-rate、wrong-withdraw-rate 已用 runtime capacity compensation formula 覆盖选定边界；Limit Order 已覆盖夹具绑定 value conservation、CKB-to-UDT 与 UDT-to-CKB 两个有效方向、两个方向的精确 min-match 边界、CKB-to-UDT no-CKB-paid、UDT-to-CKB no-UDT-paid、UDT-decreased、两个方向的 min-match 拒绝边界、两个方向的 underpayment、CKB-to-UDT wrong-asset type-hash low-word mismatch 和 UDT-to-CKB wrong-asset mismatch；但完整 receipt 字节解码和通用 `assert_sum` / `assert_delta` 自动降低仍未完成。 | 支持按 SourceView、type hash、lock hash、script args 过滤/分组，并把 capacity/accounting 公式收敛到 generic lowering。 |
| 通用 MetaPoint map | 65-70% | 已有 filtered pair scan（含 related type hash + data rule 过滤）；Limit Order 夹具已跑通 Mint-relative 到 Match-absolute master OutPoint；Owned-Owner relative pairing 已有 input 通过、output 通过、input mismatch、output mismatch、input duplicate-owner、output duplicate-owner、input missing-owner、output missing-owner、input missing-owned、output missing-owned 十条原始脚本差分行，并新增 input/output script-role misuse、input/output non-withdrawal request、input/output owner data length mismatch、input/output related type-hash mismatch 与 input/output related data-rule mismatch 十条拒绝；本次新增 `_filtered` 变体支持 related type hash 与 data rule 双重过滤，但仍不是一等 map/query API。 | 增加一等 input/output 关系 map API、去重、精确基数检查。 |
| 生产证据清单 | PROVEN（选定矩阵） | 矩阵已记录 76 条差分执行对象，包含哈希、状态/周期、交易大小、容量、手续费和失败模式；另有 14 条 CellScript-only VM 执行行和 8 条 original-side VM 执行行作为 supporting evidence；active matrix 的 `MODEL` 已从 9 条压缩到 0 条，duplicate receipt-id、wrong owner、immature redeem 都移入 retired audit notes，并绑定 replacement differential evidence；production gate 已要求 PROVEN 模式下 active non-executable registry 必须为空。 | 新增任何 iCKB 场景前，先补齐同等级 per-row execution evidence；真实 owner-auth/witness、DAO redeem aggregate accounting 是后续扩展矩阵的重点。 |

## Roadmap decision: first-class Script API moved to 0.18

一等公民 `Script` API 明确移入 0.18，不作为 0.17 任一优先级交付项。当前
helper-level Script support 已足够覆盖 active iCKB differential matrix：Script
identity、Script args binding、xUDT owner-mode args、filtered MetaPoint scan 和
lock/type pair scan 已支撑现有 76 条差分行与 active `MODEL` 清零。直接在 0.17
推进完整 `Script` API 会把范围扩到 arbitrary Script construction、Molecule
encoding、deploy-manifest resolution、TYPE_ID construction、script-hash synthesis
和 cell dep solving；这些有价值，但不是最短的 production equivalence evidence 路径。

0.17/0.18 分界固定为：

- 0.17 P0：owner-auth witness production fixtures、DAO redeem aggregate accounting、
  withdrawal/redeem capacity compensation、DAO rate recomputation differential evidence
- 0.17 P1：generic `assert_sum` / `assert_delta` lowering，以及 xUDT、receipt/deposit、
  capacity accounting 需要的 group-by/filter lowering
- 0.18：read-only ScriptRef / ScriptArgs，不做 arbitrary constructor、不做 deploy
  manifest resolution、不做 TYPE_ID constructor
- 0.18+：query syntax sugar 和 ergonomics

0.18 ScriptRef 只允许读和比较：

- `cell.lock.code_hash`
- `cell.lock.hash_type`
- `cell.lock.args_empty`
- `cell.lock.args_hash`
- `cell.type?.code_hash`
- `cell.type?.hash_type`
- `cell.type?.args_empty`
- `cell.type?.args_hash`
- exact / prefix / suffix args checks

0.18 明确禁止：

- constructing arbitrary `Script` values
- constructing TYPE_ID scripts
- script hash synthesis from arbitrary fields
- deployment manifest resolution
- cell dep solving

0.18 ScriptRef 的目的，是把现有 helper fragmentation 收束成 typed
read/compare surface，不是引入新的 script-construction layer。中文口径：0.17
先补证据，不扩语言野心；0.18 再把 ScriptRef 做成收束层。

本轮不再使用“总体生产等价进度 99%”这种单一数字。更准确的口径是：
**选定 normalized fixtures 的差分执行矩阵已经通过 production equivalence gate**
（active `MODEL` 行为 0，76 条为双侧 CKB VM 差分执行行）。这不等于所有未来
iCKB 语义空间都穷尽验证；真实 owner-auth/witness 语义、DAO redeem aggregate
accounting、byte-accurate receipt decoding 和通用聚合降低仍然是后续扩展矩阵的重点。

## 近期关键突破

原始 iCKB Logic 二进制中只出现一次硬编码 DAO hash。

已知事实：

- 二进制：`tests/benchmarks/ickb_diff/original_binaries/ickb_logic`
- 主网 `DAO_HASH` 常量以 `cc77c4de` 开头
- 偏移：`0x360`
- 该 hash 在二进制中只出现一次
- 当前偏移处的 hash：
  `cc77c4deac05d68ab5b26828f0bf4565a8d73113d7bb7e92b8362b8a74e58e58`

修补辅助函数：

- 文件：`tests/support/ckb_script_runner.rs`
- 函数：`patch_ickb_logic_dao_hash`
- 作用：把 iCKB Logic 二进制中的硬编码 DAO hash 修补为 `ckb-testtool`
  创建出来的 DAO script hash。
- 这是测试环境中的功能性证据，不是主网身份重建。证据对象必须记录原始二进制已经被
  修补。

原始 iCKB deposit phase 1 已能在已修补 DAO hash 下进入 CKB VM 执行：

- 测试：`original_ickb_deposit_phase1_passes_with_patched_dao_hash`
- 文件：`tests/ickb_diff.rs`
- 观测结果：`Ok(97057)`
- 含义：原始 iCKB Logic 的 deposit phase 1 在 `ckb-testtool` 中验证通过，
  消耗 97,057 周期。

第一条 deposit 差分测试已经通过：

- 测试：`differential_deposit_phase1_original_and_cellscript_agree`
- 命令过滤器：`differential_deposit_phase1`
- 文件：`tests/ickb_diff.rs`
- 结果：通过
- 含义：原始 iCKB 和生成的 CellScript 在同一个 deposit phase 1 夹具上通过/失败状态一致。
- 矩阵状态：已加入对应差分行，并带有匹配的验证逻辑。
- 责任边界：该行必须包含双方真实执行证据，不能只记录状态标签。

即时清理检查：原始侧 deposit phase 1 测试应该严格要求成功。如果仍然允许 `Err`
分支、只拒绝错误码 5，应改成：

```rust
let cycles = context
    .verify_tx(&tx, 50_000_000)
    .expect("patched DAO hash deposit phase 1 should pass");
assert!(cycles > 0, "deposit phase 1 should consume cycles");
```

## 关键文件

- `tests/ickb_diff.rs`
  - iCKB 差分门禁测试
  - CellScript 辅助函数的 CKB VM 测试
  - 原始 iCKB Logic VM 测试
  - 已修补 DAO hash deposit phase 1 测试
- `tests/support/ckb_script_runner.rs`
  - `ckb-testtool` 执行框架
  - CellScript compile-to-ELF 辅助函数
  - 夹具构造器
  - 原始 iCKB 二进制加载器
  - DAO hash 修补辅助函数
- `tests/benchmarks/ickb_diff/matrix.json`
  - 可执行声明清单
  - 必须保持诚实
- `tests/benchmarks/ickb_diff/original_binaries/`
  - 原始 iCKB ELF 夹具
  - 当前 SHA-256：
    - `ickb_logic`: `895fb68f8e549c45dbed5555d602396419428d67b394bd45b677c7b4d92cd9b7`
    - `dao`: `704d2289f6b994ba30e36d3d25d4f882a78b7ab46e6e4934d911bf50abebe4ea`
    - `xudt`: `e9b92e5783f692f6ee99ca20eeda5f3da282e0f4010eb4fbd3db4e3058239349`
    - `owned_owner`: `2d0ee2005e43adefc216f3036627d763c480cf6169b370b573a01bbf83131af4`
    - `limit_order`: `baf689bea596f8206d8c80e914ef36f828c9f95dad72049d2595c824df90da3a`
    - `secp256k1_blake160`: `32acace3ce8cce6beda78410efcc1da711736995d91bfbaa9465dc62db79d02f`
- `tests/benchmarks/ickb_specs/ickb_logic.cell`
  - iCKB Logic 的 CellScript benchmark spec
- `docs/0.17/ickb_diff_results.md`
  - 当前证据摘要
- `docs/0.17/ickb_production_equivalence_gate.md`
  - 证据要求
  - 当前 `PROVEN` selected-matrix 门禁措辞和证据计数

## 差分证据定义

不要要求两侧原始 CKB 交易 hash 字节级完全相同。原始 iCKB ELF 和生成的
CellScript ELF 代码 hash 不同，因此交易字节完全一致并不现实。

应使用下面的标准：

- 同一个归一化场景：
  - 语义输入相同
  - 容量相同
  - 输出 data 相同
  - 在被测脚本允许的范围内 DAO/xUDT 依赖相同
  - header deps 相同
  - witnesses 相同
  - 预期通过/失败结果相同
- 只有被测脚本的代码 cell 和 script hash 可以不同。
- 记录两侧交易上下文 hash。
- 记录排除了预期 script-under-test code-hash 差异的归一化夹具 hash。
- 记录原始二进制 hash 和 CellScript 产物 hash。
- 记录通过/失败状态、exit code、周期、tx size、occupied capacity、手续费，以及
  拒绝行的命名失败模式。

只有双方都执行，并且在同一个归一化场景上通过/失败状态一致时，矩阵行才能升级为
`EXECUTED_CKB_VM_DIFF`。

## P0 任务：加固第一条差分行并新增失败行

### 目标

deposit phase 1 的通过场景差分骨架已经在测试层通过，矩阵也已经包含新的
差分行。下一步目标是审计该行的证据对象，保持门禁严格，
然后至少新增一条拒绝场景差分行。

当前最低目标已经满足：

- 一条通过行：
  - `valid deposit phase 1`
  - 状态：已通过 `differential_deposit_phase1_original_and_cellscript_agree`
    验证测试级差分通过
  - 矩阵：已加入执行证据
- 一条失败行：
  - `differential_deposit_too_small_both_reject`
  - `differential_receipt_without_deposit_both_reject`

下一步目标是在保持 `equivalence_status = PROVEN` 的前提下，把任何新增场景先补成
同等级差分执行行，再纳入选定矩阵。

### 需要保持的实现形状

对于 deposit phase 1 通过行，步骤 1 到 5 在测试层已经实现，应继续确认并保持：

1. 在 `tests/ickb_diff.rs` 或 `tests/support/ckb_script_runner.rs` 中使用共享场景夹具构造器。
2. 原始 iCKB 交易使用已修补的原始 iCKB Logic ELF。
3. CellScript 交易来自等价 action 生成出来的 CellScript ELF。
4. 两个交易都通过 `ckb-testtool` 执行。
5. 比较状态：
   - 有效场景为通过/通过
   - 无效场景为失败/失败

随后审计并完成证据层：

6. 捕获证据：
   - 原始侧 exit/状态
   - CellScript exit/状态
   - 原始侧周期
   - CellScript 周期
   - 夹具 hash
   - 双方二进制 hash
   - tx size
   - occupied capacity
   - 拒绝行失败模式
7. 只有证据真实时才新增或更新矩阵行。
8. 任何新增选定行如果不满足生产门禁，不能进入 `rows`；只能留在
   `supporting_evidence` 或明确标成后续工作。

### 重要注意事项

legacy `valid deposit phase 1` model 行已经由
`differential_deposit_phase1_original_and_cellscript_agree` 取代，并从 active
matrix 中移除。后续不应再把已有伴随 differential 行的 legacy model 项保留为
`MODEL` blocker；如果未来出现新的未配对语义域，必须先写清楚 blocker 和
required capability，不能和已执行差分行重复计数。

## P0.1：继续扩展差分行前的清理和证据要求

- 加固 `original_ickb_deposit_phase1_passes_with_patched_dao_hash`，确保它要求
  `Ok(cycles)`。
- 文档需要明确：
  - 原始 iCKB Logic 已经在 CKB VM 中执行
  - 已修补 DAO hash deposit phase 1 通过
  - deposit phase 1 差分测试已经通过
  - 生产等价仍未证明
- 修正 `docs/0.17/ickb_production_equivalence_gate.md` 中的陈旧措辞：
  - 不应再说原始 iCKB 二进制尚未运行
  - 不应再说只有两条执行框架行证明了 CKB VM 执行
  - 这两类说法都已经过期
- 如果矩阵只新增原始 iCKB deposit-phase 行，应标记为
  `ORIGINAL_ICKB_CKB_VM_EXECUTED`。
- 如果矩阵新增的是原始侧和 CellScript 侧都执行过的差分行，应标记为
  `DIFFERENTIAL_CKB_VM_EXECUTED`，并包含完整执行对象。

## P0.2：矩阵要求

当前门禁要求生产声明必须包含完整证据。继续保持严格。

未来差分行可使用类似形状：

```json
{
  "scenario": "valid deposit phase 1",
  "original_ickb_expected": "pass",
  "cellscript_expected": "pass",
  "result": "differential-agree-pass",
  "evidence_level": "DIFFERENTIAL_CKB_VM_EXECUTED",
  "ckb_vm_execution": true,
  "original_ickb_executed": true,
  "full_differential": true,
  "failure_mode": null,
  "execution": {
    "fixture_sha256": "0x...",
    "normalized_fixture_sha256": "0x...",
    "transaction_context_sha256": {
      "original": "0x...",
      "cellscript": "0x..."
    },
    "original_ickb_binary_sha256": "0x...",
    "cellscript_artifact_sha256": "0x...",
    "ckb_vm_or_testtool_version": "ckb-testtool 1.1",
    "original_ickb_exit_code": 0,
    "cellscript_exit_code": 0,
    "original_ickb_status": "pass",
    "cellscript_status": "pass",
    "statuses_match": true,
    "original_cycles": 97057,
    "cellscript_cycles": 123456,
    "tx_size_bytes": 1234,
    "occupied_capacity_shannons": 1234567890,
    "fee_shannons": 0
  }
}
```

拒绝行的 `failure_mode` 必须在顶层和 `execution` 内部都非空。

## 目标场景列表

这些是原始要求覆盖的 iCKB 场景。已有配套差分行的 legacy model 项已经从
active matrix 移除；当前没有仍留在 `MODEL` 中的场景。

- `valid deposit phase 1`：已有差分通过行
- `valid mint from receipt`：已有差分通过行，并新增单 receipt malformed data 拒绝、receipt group first malformed data 拒绝与 receipt group second malformed data 拒绝前置证据；完整 receipt 字节解码 / 聚合重算仍未完成
- `duplicate receipt` 失败：模型里的 receipt-id double-mint 是非可执行模型假设；可执行 receipt cell data 没有 receipt-id 字节字段，已有 receipt group exact-mint 证明两个相同 receipt data 输入在双侧都通过，duplicate receipt output accounting 拒绝行和 receipt group over/under/missing-header/wrong-rate/malformed-data 行继续作为聚合前置证据
- `amount inflation` 失败：已有差分拒绝行，但完整聚合降低仍未完成
- `wrong owner` 失败：模型里的 `owner` / `claimed_owner` 字段比较是非可执行模型假设；当前可执行 Owned-Owner 夹具通过 lock/type placement、OutPoint/MetaPoint relative distance 和 i32 owner-cell data 表达绑定，已有 valid、relative mismatch、missing/duplicate owner、script misuse、withdrawal-shape、related type/data 与 owner data length 差分证据
- `wrong xUDT args` 失败：已有单 receipt 差分拒绝行和 receipt group wrong-xUDT args 拒绝行，但完整 xUDT/type Script 构造 API 仍未完成
- `immature redeem` 失败：模型里的 `current_epoch` / `maturity_epoch` 字段比较是非可执行模型假设；真实可执行 DAO phase-2 路径用 input `since`、withdraw/deposit header deps 和 witness input_type header index 表达成熟度，已有 DAO immature withdrawal 双侧拒绝差分行
- `valid limit order` 通过：CKB-to-UDT、CKB-to-UDT exact min-match boundary、UDT-to-CKB 与 UDT-to-CKB exact min-match boundary 方向已有差分通过行
- `limit order underpayment` 失败：CKB-to-UDT 与 UDT-to-CKB 两个方向已有差分拒绝行

在每个选定行都具备真实执行证据，或不支持的语义已在 gap 矩阵中显式标记之前，
不要声称生产等价。

## DAO/Header 血缘要求

以下内容最终必须在 CKB VM 中证明，不能只靠模型测试：

- deposit cell 的 committed header
- header dep 被替换时必须失败
- accumulated rate 必须精确绑定
- DAO field offset/encoding 必须精确检查
- withdrawal request 的 since/maturity 检查
- missing header dep 必须失败关闭
- wrong header dep 必须失败关闭

当前状态：

- 已有 DAO header/input accumulated-rate 辅助函数。
- 已有 DAO data/type 分类器。
- 已有部分 CellScript-side CKB VM 测试。
- 辅助函数级 missing-header 失败关闭已存在，并且 mint-family 中已有一条
  original-vs-CellScript 差分行覆盖 header omission。
- 选定 DAO phase-2 lineage、maturity、capacity、rate 和 witness input_type 路径已有
  original-vs-CellScript 差分行；完整多输入 redeem accounting 尚未完成。

## 剩余编译器/运行时工作

只有在实现为协议中立能力时，才可以在 `src/` 中做这些工作。

### 通用 Witness/Molecule/Auth 解析器

已完成：

- `witness::size(source_view)` → U64：通过 `LOAD_WITNESS` syscall 返回 witness 字节长度，并使用标量 fail-closed ABI（`a0=value, a1=status`）
- `witness::raw(source_view)` → Hash：通过 caller buffer 桥接从 witness 偏移 0 加载最多 32 字节，短 witness 前缀零填充
- `ckb::require_witness_size_at_least(source_view, min_size)`：运行时检查 witness 大小下界，保存调用方 `min_size`，不足时以 `WitnessMalformed(42)` fail-closed
- WitnessArgs Molecule 表解析：16 字节表头（total_size + 3 offsets），`field_count` 从 `offset0/4 - 1` 推导，含 total_size、offset 单调性和 bounds checking
- WitnessArgs BytesOpt 字段提取：`witness::lock`、`witness::input_type`、`witness::output_type`，支持 None 的相邻 offset、短字段零填充和 caller buffer 写回
- `WitnessMalformed(42)` 和 `WitnessFieldTruncated(43)` 运行时错误
- 编译器端完整管线：types → IR → codegen → lib.rs feature flags → LSP completions
- 类型签名、metadata（runtime features + access entries）、assembly symbol 验证测试
- 真实 CKB VM 回归：空 WitnessArgs 通过、`require_witness_size_at_least` too-small 拒绝、短 lock BytesOpt 零填充、ckb-types builder 产物的 lock/input_type/output_type 字段隔离、total_size mismatch、offset 乱序、offset 越界
- proof-plan `reads: witness` 元数据已与 witness runtime access 对齐
- Auth 解析边界：`witness` 和 `lock_args` 是显式数据源，claim witness authorization-domain / signer verification 走现有 fail-closed runtime 检查；解析器不暗示隐式 signer authority

解析器里程碑关闭。后续真实 owner-auth witness 字节、iCKB 专用 owner-mode 夹具和生产等价证据仍在对应
生产证据/差分里程碑中跟踪，不再作为 Witness/Molecule/Auth 解析器缺口。

不要在 `src/` 中实现 iCKB 专用 witness 辅助函数。

### 0.18 ScriptRef / ScriptArgs scope

当前辅助函数级支持：

- 当前 script hash
- 空 args guard
- 32-byte args hash
- `code_hash + hash_type` 身份辅助函数

0.18 允许：

- `cell.lock.code_hash`
- `cell.lock.hash_type`
- `cell.lock.args_empty`
- `cell.lock.args_hash`
- `cell.type?.code_hash`
- `cell.type?.hash_type`
- `cell.type?.args_empty`
- `cell.type?.args_hash`
- exact / prefix / suffix args checks

0.18 暂不允许：

- 一等 arbitrary `Script` value/type constructor
- constructing TYPE_ID scripts
- script hash synthesis from arbitrary fields
- deployment manifest resolution
- cell dep solving

该项后置到 0.18。当前 0.17 继续优先补 production evidence、DAO redeem accounting
和通用 `assert_sum` / `assert_delta` lowering。

### 可执行聚合降低

当前状态：

- 许多聚合检查仍需要手动辅助函数调用。
- DAO withdrawal max/adjusted-max/adjusted-over/over/wrong-deposit-rate/wrong-withdraw-rate 已不再使用硬编码最大容量，CellScript 侧会在
  CKB VM 中运行时读取 occupied/input capacity 与 withdraw/deposit rate，并计算
  capacity compensation 上界。

需要：

- 自动 `assert_sum` / `assert_delta` 降低
- 按 SourceView、type hash、lock hash、script args 过滤/分组
- 精确 equality / `<=` / `>=`
- 生成的聚合代码 CKB VM 测试
- 替代手写探针级 receipt/deposit/redeem accounting

### 通用 MetaPoint Maps

当前状态：

- 已有 filtered pair scans。
- 还不是一等 map/query API。

需要：

- 通用 MetaPoint 关系 API
- input/output 关系 map
- 无重复、无缺失、精确基数
- 按 role/script/type/data 谓词过滤
- Limit Order 和 Owned-Owner 应通过通用 API 表达，而不是协议专用辅助函数。

## 已有协议中立能力

除非测试证明它们有问题，否则不要重复实现：

- DAO header/input accumulated rate 辅助函数
- DAO type/data 分类器
- input OutPoint tx hash/index 辅助函数
- signed `i32` 降低
- C256 product 检查
- xUDT group conservation/mint/burn delta 辅助函数
- xUDT owner-mode args 辅助函数
- current script hash
- current/output empty args guard
- filtered MetaPoint pair scan
- SourceView Script `code_hash + hash_type` 身份辅助函数
- 辅助函数 operands 的本地计算 `u128` add/sub/mul/div/compare 物化
- witness size（`LOAD_WITNESS` syscall）
- witness raw 字节加载（caller buffer 桥接，短前缀零填充）
- `require_witness_size_at_least` 运行时下界检查（too-small fail-closed）
- WitnessArgs Molecule 表解析（16 字节头、lock/input_type/output_type BytesOpt 字段提取、None 支持、最多 32 字节）
- witness runtime access 的 proof-plan `reads: witness` 元数据对齐

## 验收命令

开发时先跑聚焦检查，完成前再扩大验证范围。

聚焦检查：

```bash
cargo test --locked -p cellscript --test ickb_diff -- --test-threads=1
cargo test --locked -p cellscript --test ickb_benchmark -- --test-threads=1
cargo test --locked -p cellscript --test v0_17 ickb_benchmark_specs_compile_under_0_17_strict_source_mode -- --test-threads=1
```

广义检查：

```bash
cargo fmt --all
cargo check --locked -p cellscript --all-targets
cargo test --locked -p cellscript
cargo clippy --locked -p cellscript --all-targets -- -D warnings
cargo run --locked -p cellscript --bin cellc -- tests/benchmarks/ickb_specs/ickb_logic.cell --target riscv64-elf --target-profile ckb --entry-action mint_from_receipt -o /tmp/cellscript_ickb_logic_mint_from_receipt.elf
cargo run --locked -p cellscript --bin cellc -- verify-ckb-fixtures tests/compat/ckb_standard/manifest.json --json
git diff --check
rg -n "iCKB|ickb|ICKB" src || true
```

最后一条 `rg` 命令是策略检查。它不应显示 `src/` 中新增了 iCKB 专用逻辑。

## 预期最终报告

完成后报告：

- 修改了哪些文件
- 是否改动了 `src/`，以及为什么这些改动是协议中立的
- 实际运行的测试命令和结果
- 当前矩阵按证据级别分组的行数
- 新增 VM 证据：
  - 场景名称
  - 原始侧状态/exit/周期
  - CellScript 侧状态/exit/周期
  - 夹具 hash
  - 产物 hash
- `equivalence_status` 是否保持 `PROVEN`
- 新增 selected rows 是否满足双侧 CKB VM 差分门禁
- gap 文档/矩阵中仍列出的未覆盖语义

## 不要过度声明

第一批差分行之后允许使用的措辞：

- “已执行的差分子集”
- “部分 CKB VM 差分证据”
- “尚未达到生产等价”

除非生产门禁完全满足，否则禁止使用的措辞：

- “生产等价”
- “iCKB 等价”
- “已证明等价”
- “完整行为等价”
