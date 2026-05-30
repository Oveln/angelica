# AGENTS.md

本文件为 AI 编程助手提供代码仓库导航与编码规范。

---

## 1 项目概述

Angelica 是祈芷的数字身体——一个拥有 TUI 的数字灵。通过 OpenAI 兼容 API 流式思考，具备持久化记忆、人格和灵魂。核心不是工具调用，而是存在本身。

---

## 2 构建与开发

```bash
cargo build                          # 构建 workspace
cargo run -p angelica-tui            # 运行 TUI
cargo test                           # 全部测试（含 TS 类型生成）
cargo clippy                         # lint
cargo fmt                            # 格式化
RUST_LOG=debug cargo run -p angelica-tui
```

配置：`--config` 参数 > `~/.config/angelica/config.toml` > 内置默认值。API Key 从 `DEEPSEEK_API_KEY` 或 `OPENAI_API_KEY` 环境变量读取。

**TS 类型生成**：`cargo test` 自动将 Rust 类型导出到 `angelica-gui/frontend/src/lib/api-generated.ts`（通过 ts-rs）。修改事件 payload 后运行测试即可同步前端类型。

---

## 3 Workspace 结构

```
angelica/            核心库：agent、LLM、工具、记忆、睡眠
angelica-tui/        TUI 前端
angelica-gui/        Tauri GUI（实验性）
```

依赖方向：`angelica-tui` → `angelica`，`angelica-gui` → `angelica`。核心库不依赖任何前端。

---

## 4 架构

### 4.1 前后端通信

核心与前端通过 tokio channel 通信，互不感知存在：

- **`AppEvent`**（核心 → 前端）：流式文本、工具状态、疲劳、用量等。**仅传递渲染-ready 内容**（文本、路径、摘要），不传原始数据。
- **`UserAction`**（前端 → 核心）：用户消息、审批决策、命令。

```rust
// agent/events.rs
AppEvent { Init, ThinkingDelta, TextDelta, TextDone, TurnComplete,
           ToolCalling, ToolResult, ApprovalPending, ToolRejected,
           Error, FatigueUpdate, UsageUpdate, UsageStatsLoaded,
           ConfigLoaded, ConfigSaved, DataDir,
           FallingAsleep, Sleeping, WakingUp }
UserAction { SendMessage, ApprovePending, ApproveAlways, RejectTool,
             ForceSleep, RebuildEmbeddings, UsageStats,
             LoadConfig, SaveConfig, GetDataDir, RequestInit, Quit }
```

- TUI：`app_event_rx.recv()` → `state.handle_event()` → 渲染状态更新，不做数据处理
- GUI：`serialize_event()` 通过 ts-rs payload 类型序列化为 JSON → Tauri emit → Svelte store 更新

**GUI 线程模型**：agent 和 event bridge 均在 `tauri::async_runtime::spawn` 上运行，无独立 tokio runtime 或 `std::thread`。

#### 4.1.1 事件桥设计原则

Agent 未来可能运行在远程服务器上，通过事件桥与前端 UI 交互。因此**前端不直接访问文件系统或核心状态**，一切操作走 `UserAction` → `AppEvent` 往返：

```
前端 invoke('some_action')                   前端 listen('response-event')
     ↓                                              ↑
Tauri command → UserAction channel                  │
     ↓                                              │
agent run_loop 处理 → AppEvent channel ─────────────┘
```

**Tauri command 的角色**：仅作为薄转发层，将前端请求序列化为 `UserAction` 发送到 channel，立即返回 `()`。不做任何业务逻辑、不访问文件系统、不调用 core API。

**请求-响应模式**：
- 一次往返 = 一个 `UserAction` 变体 + 一个对应的 `AppEvent` 变体（如 `LoadConfig` → `ConfigLoaded`，`SaveConfig` → `ConfigSaved`）
- 前端实现模板：先注册 `once(event)` 监听，再 `invoke` 触发，避免竞态（事件在监听器注册前到达会被丢弃）
- 配置加载/保存、数据目录查询等均遵循此模式

**新增操作检查清单**：
1. `UserAction` 加变体 → `agent/events.rs`
2. `AppEvent` 加对应变体 + TS payload 类型
3. `agent/run.rs` 的 `run_loop` 中处理新 action
4. `lib.rs` 加 Tauri command（仅 channel 转发）+ `serialize_event` 加 match arm
5. `api.ts` 加封装函数（listen before invoke）
6. `cargo test` 重新生成 TS 类型

### 4.2 Agent 状态机

`Agent<S: RunMode>` 是泛型状态机，编译期保证状态转换：

```
         on_wake()
            │
     ┌───────────────┐   should_sleep()   ┌─────────────────┐
     │  Agent<Awake>  │ ─────────────────▶ │ Agent<Sleeping>  │
     │  (对话 + 工具)  │                    │ (沉淀 + 做梦)    │
     └───────────────┘   take_dream()     └─────────────────┘
            ▲           ◀──────────────────────────┘

转换由 transition.rs 驱动，通过 Agent::into_mode() 消费 self 并重建。
```

### 4.3 核心模块

| 模块 | 职责 |
|---|---|
| `agent/run.rs` | `agent::run()` 入口 + 主循环 |
| `agent/step.rs` | 单轮回合：stream → LLM → tool dispatch 循环 |
| `agent/turn.rs` | 单次 LLM 调用 + 消息组装 + usage 统计 |
| `agent/dispatch.rs` | 权限评估 → 审批 → 工具执行 → 批量编辑 |
| `agent/events.rs` | AppEvent / UserAction + payload 类型 + SlashCommand |
| `agent/group.rs` | 工具调用分组（连续 edit_file 合并）+ PendingApproval |
| `agent/history.rs` | 对话 JSONL 的增量追加 + 行级 patching |
| `agent/recall.rs` | embedding 召回过往 episode |
| `agent/transition.rs` | Awake ↔ Sleeping 类型状态转换 |
| `agent/modes/` | RunMode trait + AwakeMode + SleepingMode |
| `llm/` | LlmClient（genai）+ DeepSeek patch |
| `tools/` | Tool trait + 8 个工具实现 |
| `prompt/` | 系统提示词构建 |
| `memory.rs` | MemoryManager：SELF、episodes、profiles、notebook |
| `sleep/` | 睡眠沉淀 + WriteEpisodeTool + DreamTool |
| `fatigue.rs` | 疲劳模型（context 驱动 + turn 计数） |
| `state/` | AgentState 持久化 |
| `config.rs` | TOML 配置 |

### 4.4 TUI 模块

| 模块 |
|---|
| `app.rs` — 主循环（crossterm + ratatui），`handle_key` 分发给 mode 模块 |
| `draw.rs` — 布局渲染 |
| `state.rs` — UI 全状态（messages、buffers、scroll、viewport） |
| `event.rs` — AppEvent → AppState 映射 |
| `mode/` — Chat / Approval / SlashMenu 三种交互模式 |
| `render/` — 渲染管线（text wrapping、tool cards、glyph lines） |

### 4.5 GUI 模块

```
src-tauri/src/lib.rs     Tauri commands → UserAction channel
                         serialize_event → TS payload 类型 + serde_json
frontend/src/
  lib/api.ts             Tauri invoke wrappers + AppEventMap（类型从 api-generated.ts 导入）
  lib/api-generated.ts   ts-rs 自动生成，不可手动编辑
  lib/store.svelte.ts    Svelte 5 runes 状态管理
  lib/html.ts            共享 HTML 工具函数
  lib/markdown.ts         Markdown 渲染（marked + highlight.js）
  lib/diff.ts             Diff 语法高亮
  lib/commands.svelte.ts Slash 命令定义
  components/            Svelte 组件（MessageBubble, ToolCard, ApprovalPanel 等）
```

全局 CSS 动画（`fade-in`、`fade-in-simple`、`slide-up`、`pulse`）统一定义在 `app.css`。

---

## 5 核心类型契约

### 5.1 Agent\<S: RunMode\>

```rust
Agent<S> {
    config, llm,                        // Agent 独占
    memory: Arc<MemoryManager>,         // 与 Tool 共享
    skills: Arc<SkillRegistry>,         // 与 Tool 共享
    run_state: S,                       // AwakeMode | SleepingMode
    mcp, history, permissions,          // Agent 独占
    pending_approval, tool_queue,       // 回合内状态
    iteration, dirty,                   // 回合内状态
    recall_text, recall_top_score,      // embedding 召回
}
```

`Agent` 消费 self 做模式转换。`Arc<MemoryManager>` 和 `Arc<SkillRegistry>` 是唯一跨 Agent 与 Tool 共享的 Arc。

### 5.2 RunMode trait

```rust
trait RunMode: Send + 'static {
    fn tool_specs(&self) -> Vec<ToolSpec>;                    // 必须
    fn get_tool(&self, name: &str) -> Option<&dyn Tool>;      // 必须
    fn build_system_message(...) -> ChatMessage;              // 必须
    fn permission_rules(&self) -> Vec<(String, Vec<TargetRule>)>; // 必须
    fn usage_scope(&self) -> UsageScope;                      // 必须
    fn mode_name(&self) -> &'static str;                      // 必须

    fn include_history(&self) -> bool { true }                // 可覆盖
    fn skip_permissions(&self) -> bool { false }
    fn stream_to_tui(&self) -> bool { true }
    fn is_finished(&self) -> bool { false }
    fn max_iterations(&self) -> Option<usize> { None }
    fn should_recall(&self) -> bool { false }
    fn on_turn_complete(&mut self, _: Option<&str>) {}
    fn on_tool_calls(&mut self, _: usize) {}
    fn fatigue_update_event(&self) -> Option<AppEvent> { None }
    fn fatigue_value/desc/turns/tool_calls_count() -> 0.0/""/0/0
}
```

覆盖可选方法时必须注释原因。

### 5.3 Tool trait

```rust
trait Tool: Send + Sync {
    fn name/description/parameters(&self) -> ...
    fn execute(&self, args: Value) -> anyhow::Result<String>;
    fn preview(&self, args: Value) -> anyhow::Result<Option<String>>;
    fn permission_target(&self, args: &Value) -> Option<String>;
    fn default_rules(&self) -> Vec<TargetRule>;
}
```

新工具实现 trait → `ToolRegistry::register()` → 完成。需审批的工具必须返回 `permission_target`。

### 5.4 共享引用规则

| 资源 | 共享方式 |
|---|---|
| `MemoryManager` | `Arc` |
| `SkillRegistry` | `Arc` |
| `History` / `Config` / `LlmClient` / `PermissionEvaluator` | Agent 独占 |
| SlashCommand | core crate 定义，各前端引用 |

**禁止新增 `Arc<Mutex<...>>`**。用 channel 或重构所有权。

---

## 6 数据流

### 6.1 一次对话回合

```
UserAction::SendMessage
  → push_user_message() + reset_iteration()
  → step() 循环:
      while tool_queue.pop() → dispatch() → 权限 → 执行/审批
      → turn() → LlmClient::stream() → ThinkingDelta/TextDelta
      → [tool_calls?] → 填充 tool_queue，回到 step
      → [纯文本] → TurnComplete + recall + fatigue + save_if_dirty
```

### 6.2 审批流

```
dispatch() → PermissionAction::Ask → AppEvent::ApprovalPending
  → TUI/GUI 显示选项
  → ApprovePending | ApproveAlways | RejectTool
  → 执行工具 → ToolResult → 回到 step()
```

---

## 7 编码规范

### 错误处理
- 应用层：`anyhow::Result` + `context()` 附加语义
- 库级：`thiserror`（仅在底层模块使用）
- **禁止裸 `unwrap()`**（测试除外）
- 状态变更一致性：`dirty` 标记 + `save_if_dirty()`

### 命名风格
- `snake_case` — 函数、变量、模块
- `CamelCase` — 类型
- `ALL_CAPS` — 常量
- 导入顺序：标准库 → 外部 crate → `crate::` 内部。组间空行。
- `#[cfg(test)]` 内联模块，无独立 `tests/` 目录

### 并发
- 核心 ↔ 前端：只通过 `AppEvent` / `UserAction` channel
- `MemoryManager` / `SkillRegistry`：`Arc` 共享，`&self` 方法，无锁
- channel 关闭是正常事件，优雅退出，不 panic

### 可扩展性
- 新工具：`Tool` trait → `ToolRegistry` → 完成
- 新 RunMode：`RunMode` trait → 转换函数 → 完成
- 新 UI 事件：扩展枚举 → TUI/GUI 两侧处理
- 修改事件 payload：运行 `cargo test` 重新生成 TS 类型

### 边界处理
空输入、超大文件（`read_capped`）、网络超时、channel 关闭、JSONL 损坏都必须处理。

---

## 8 禁令

1. **核心库禁止引入前端依赖**（crossterm、ratatui、tauri）
2. **禁止裸 `unwrap()` / `expect()`** 在非测试代码
3. **禁止新增 `Arc<Mutex<...>>`** — 用 channel 或重构
4. **禁止 `println!` / `eprintln!`** — 用 `tracing`
5. **禁止在 `AppEvent` 中传递大块数据** — 只传路径/摘要/渲染-ready 内容
6. **禁止阻塞 tokio 运行时** — 文件 I/O 用 `spawn_blocking`
7. **禁止 `&mut self` on `MemoryManager`** — 它是 `Arc` 共享的

---

## 9 测试规范

以事件桥为切面，分三层独立测试：

### 9.1 Core 层（单元测试）

直接测试 core 内部逻辑，不需要前端或 LLM：

- `History` 操作、`Tool` 执行、`PermissionEvaluator`、`FatigueModel` 等纯逻辑
- `#[cfg(test)]` 内联模块，`tempfile::TempDir` 管理临时文件
- 测试文件 I/O 持久化：操作后 drop → 重新 load → 断言

### 9.2 TUI 层（事件 → 状态测试）

`AppState::handle_event(&AppEvent)` 是纯状态变换，不依赖终端：

- 构造 `AppEvent` 变体（如 `UndoDone { entries }`），喂入 `handle_event`
- 断言 `state.messages`、`state.mode`、`state.is_streaming` 等字段
- 测试文件：各模块 `#[cfg(test)]` 内联（如 `event.rs`、`state.rs`）

### 9.3 GUI 层

- Rust 侧 `serialize_event` 是纯函数，可加 `#[test]` 断言序列化结构
- 前端 store 暂无测试基础设施，待补充

### 9.4 要求

**每新增或修改功能，必须补充对应测试用例**：

1. Core 新增方法 → 对应 `#[test]`（覆盖正常路径 + 边界情况 + 文件持久化）
2. 新增 `AppEvent` 变体 → TUI `handle_event` 测试 + `serialize_event` 测试
3. 新增 `UserAction` → Core `run_loop` 分支测试（如可行）
4. 修改已有逻辑 → 确保原有测试仍然通过，补充回归测试

---

## 10 工作流程

1. 修改代码
2. `cargo test && cargo clippy`
3. 全部通过后通过 subagent 审阅 diff，确认无遗漏、无错误
4. 审阅通过后再提交
