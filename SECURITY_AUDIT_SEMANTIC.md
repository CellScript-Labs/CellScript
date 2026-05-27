# CellScript 编译器语义分析层安全审计报告

## 审计范围
- 类型检查 (`src/types/mod.rs`)
- 控制流检查 (`src/flow/mod.rs`)
- 模块解析 (`src/resolve/mod.rs`)
- 包管理 (`src/package/mod.rs`)
- 编译入口 (`src/lib.rs`)
- LSP 集成 (`src/lsp/mod.rs`)

---

## 高严重问题

### 1. `check` 与 `check_with_resolver` 路径不一致 — 跨模块导入类型注册被跳过

**位置：**
- `src/types/mod.rs:562-565` (`register_imported_type_ids`)
- `src/types/mod.rs:6396-6399` (`check` 函数)
- `src/types/mod.rs:6401-6404` (`check_with_resolver` 函数)
- `src/lib.rs:3725-3735` (`compile` 函数)
- `src/lib.rs:3739-3751` (`compile_metadata` 函数)
- `src/lsp/mod.rs:207` (LSP 诊断)

**描述：**
`TypeChecker::register_imported_type_ids` 在 `resolver` 为 `None` 时直接返回 `Ok(())`，跳过导入类型注册：

```rust
fn register_imported_type_ids(&self, seen_type_ids: &mut HashMap<String, Span>) -> Result<()> {
    let (Some(resolver), Some(module_name)) = (self.resolver, self.current_module.as_deref()) else {
        return Ok(());  // <-- 直接跳过，不注册导入类型
    };
    // ... 实际的导入类型注册逻辑 ...
}
```

以下调用路径均使用 `types::check()`（无 resolver），导致跨模块导入的类型 ID 不被注册：

1. **`compile(source, options)`** (`src/lib.rs:3725`): 单文件/REPL 编译入口，传入 `None` 作为 resolver
2. **`compile_metadata(source, target)`** (`src/lib.rs:3739`): Metadata 生成入口，调用 `types::check(&ast)`
3. **LSP 诊断** (`src/lsp/mod.rs:207`): 编辑器实时诊断，调用 `crate::types::check(&ast)`

**影响：**
- 在单文件编译、REPL、LSP 编辑器等场景中，使用 `use` 导入的其他模块类型可能产生类型 ID 冲突或类型检查遗漏
- `compile_metadata` 被外部工具调用时，可能生成不准确的 metadata

**建议：**
- 统一使用 `check_with_resolver`，或在 `check` 路径中构建一个基本的模块解析器
- 对于 LSP，应构建基于当前项目结构的 `ModuleResolver`

---

## 中严重问题

### 2. 泛型类型解析存在注入风险

**位置：** `src/types/mod.rs:5485-5504`

**描述：**
`validate_named_type` 使用简单的字符串操作提取泛型基名：

```rust
let base_name = name.split('<').next().unwrap_or(name);
if name.contains('<') && base_name != "Vec" {
    return Err(CompileError::new(...));
}
```

**问题：**
- 如果类型名中包含嵌套的 `<`（如 `Foo<Bar<Baz>>`），`base_name` 只会提取到 `Foo`，但后续的 `name.contains('<')` 仍然为真
- 更关键的是，这种解析方式容易被绕过或产生歧义

**建议：**
- 使用 AST 级别的类型解析，而非字符串分割
- 对泛型参数进行递归验证

---

### 3. Git 命令参数注入风险

**位置：**
- `src/package/mod.rs:517-519` (`git_clone_args`)
- `src/package/mod.rs:521-523` (`git_fetch_ref_args`)
- `src/package/mod.rs:525-527` (`git_checkout_args`)

**描述：**
虽然使用了 `--` 分隔符来防止选项注入，但 `url` 和 `ref_str` 仍然直接传入命令参数：

```rust
fn git_clone_args(url: &str, target: &Path) -> Vec<OsString> {
    vec![OsString::from("clone"), OsString::from("--"), OsString::from(url), target.as_os_str().to_os_string()]
}
```

**问题：**
- 恶意构造的 URL（包含特殊字符或换行）仍可能导致未预期的行为
- `ref_str` 如果包含 shell 元字符，虽然 `--` 可防止选项注入，但某些 Git 版本或配置仍可能存在解析问题

**建议：**
- 对 URL 和 ref_str 进行白名单验证
- 考虑使用 `git2` 库替代命令行调用

---

## 低严重问题

### 4. Flow 检查中 `consume`/`transfer` 仅匹配 `Identifier` 表达式

**位置：** `src/flow/mod.rs:119-136`

**描述：**
在 `collect_state_context_from_expr` 中，Consume 和 Transfer 仅在表达式是 `Identifier` 时才记录 consumed 类型：

```rust
Expr::Consume(consume) => {
    if let Expr::Identifier(name) = consume.expr.as_ref() {
        if let Some(ty) = context.variable_flow_types.get(name) {
            context.consumed_flow_types.insert(ty.clone());
        }
    }
    // ...
}
```

**问题：**
- 如果 consume/transfer 的目标不是简单标识符（如 `consume(foo.bar)` 或 `consume(array[0])`），`consumed_flow_types` 不会被更新
- 这可能导致 `validate_state_transition_create` 中 `updates_existing` 判断错误，从而漏报 "initial create must use statically known declared state" 错误

**建议：**
- 扩展 `collect_state_context_from_expr` 以处理 FieldAccess、Index 等表达式变体

---

### 5. `resolve_type_global` 等函数中的 `unwrap_or` 使用

**位置：**
- `src/resolve/mod.rs:248` (`resolve_type_global`)
- `src/resolve/mod.rs:259` (`resolve_function_global_with_module`)
- `src/resolve/mod.rs:269` (`resolve_constant_global`)

**描述：**
```rust
let symbol = name.rsplit("::").next().unwrap_or(name);
```

**分析：**
- `rsplit` 总是返回至少一个元素，因此 `unwrap_or` 永远不会使用默认值
- 虽然逻辑安全，但代码意图不够清晰，应使用 `expect` 或注释说明为何安全

**建议：**
- 替换为 `.expect("rsplit always yields at least one element")` 或添加注释

---

## 正面发现

### 6. 生产代码无 `unwrap`/`expect`

**分析：**
- `src/types/mod.rs` 中有 33 个 `unwrap`/`expect`，全部位于 `#[cfg(test)]` 测试代码中（行号 6414 以后）
- `src/resolve/mod.rs` 中有 8 个 `unwrap`/`expect`，全部位于测试代码中（行号 394 以后）
- `src/flow/mod.rs` 中没有 `unwrap`/`expect`

**结论：** 生产代码路径中没有 panic 风险，错误处理统一使用 `Result` 传播。

---

### 7. 无 `unsafe` 代码块

**分析：**
- `src/types/mod.rs`: 0 个 `unsafe`
- `src/flow/mod.rs`: 0 个 `unsafe`
- `src/resolve/mod.rs`: 0 个 `unsafe`

**结论：** 语义分析层完全不依赖 `unsafe`，内存安全由 Rust 编译器保证。

---

### 8. 循环导入防护有效

**位置：**
- `src/package/mod.rs:313-350` (`resolve_dependency_from_root`)
- `src/package/mod.rs:560-568` (`check_circular_deps`)

**分析：**
- `resolve_dependency_from_root` 使用 `stack` 参数在解析过程中实时检测循环依赖
- `check_circular_deps` 构建完整的依赖图并使用 DFS 检测循环
- 双重防护机制确保循环依赖被捕获

---

### 9. 路径遍历防护有效

**位置：**
- `src/package/mod.rs:640-659` (`canonical_package_child_path`)
- `src/package/mod.rs:661-673` (`reject_package_path_escape`)
- `src/package/mod.rs:681-713` (`ensure_git_cache_child`)

**分析：**
- `reject_package_path_escape` 拒绝空路径、绝对路径、包含 `..` 或根目录组件的路径
- `canonical_package_child_path` 对候选路径进行 `canonicalize` 后验证其是否在基础根目录内
- `ensure_git_cache_child` 对 git 缓存目录进行类似的限制

---

## 总结

| 严重程度 | 数量 | 类别 |
|---------|------|------|
| 高 | 1 | 路径不一致导致的安全漏洞 |
| 中 | 2 | 泛型解析和命令注入 |
| 低 | 2 | 检查覆盖率和代码清晰度 |
| 正面 | 4 | 无 unsafe、无生产 unwrap、循环/路径防护有效 |

**最优先修复：** 统一 `check` 和 `check_with_resolver` 路径，确保所有编译入口（尤其是 `compile_metadata` 和 LSP）都能正确处理跨模块导入的类型。
