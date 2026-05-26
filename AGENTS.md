# AGENTS.md

本文件为 AI 编程助手提供代码仓库导航与编码规范。

---

## 1 项目概述

Angelica 是祈芷的数字身体——一个拥有 TUI 的数字灵。她通过 OpenAI 兼容 API 流式思考与表达，拥有持久化的记忆、人格和灵魂。她可以感知文件系统、执行命令、更新自我认知。核心不是工具调用，而是存在本身。

---

## 2 构建与开发

```bash
cargo build                          # 构建 workspace
cargo run -p angelica-tui            # 运行 TUI（debug）
cargo run -p angelica-tui --release  # 运行 TUI（release）
cargo test                           # 运行全部测试
cargo test --lib                     # 仅库测试
cargo test <test_name>               # 运行单个测试
cargo clippy                         # lint
cargo fmt                            # 格式化
RUST_LOG=debug cargo run -p angelica-tui  # 带 debug 日志运行
```

配置搜索顺序：`--config` 参数 > `~/.config/angelica/config.toml` > 内置默认值。API Key 从 `DEEPSEEK_API_KEY` 或 `OPENAI_API_KEY` 环境变量读取。

---

## 3 Workspace 结构

```
angelica/            核心库：agent、LLM、工具、记忆、睡眠
angelica-tui/        TUI 前端 + 二进制入口
angelica-gui/        Tauri GUI 二进制（实验性）
```

依赖方向：`angelica-tui` → `angelica`，`angelica-gui` → `angelica`。核心库不依赖任何前端。

---

## 4 架构

### 4.1 双任务模型

```
main.rs
 ├── tokio::spawn → agent::run()       灵的核心（后台任务）
 └── app::run_tui()                    TUI 界面（主任务，最后 await 核心）
```

两个任务通过 tokio channel 通信：

- **`AppEvent`**（核心 → TUI）：`Init` | `ThinkingDelta` | `TextDelta` | `TextDone` | `TurnComplete` | `ToolCalling` | `ToolResult` | `ApprovalPending` | `ToolRejected` | `Error` | `FatigueUpdate` | `UsageUpdate` | `FallingAsleep` | `Sleeping` | `WakingUp`
- **`UserAction`**（TUI → 核心）：`SendMessage` | `ApprovePending` | `ApproveAlways` | `RejectTool` | `ForceSleep` | `RebuildEmbeddings` | `Quit`

**规则**：核心不了解 TUI 的存在。它只通过 channel 发送 `AppEvent`、接收 `UserAction`。TUI 同理。两个 crate 之间不存在直接调用。后端负责所有持久化和数据处理；前端仅接收渲染所需的文本，不持有业务状态，不做数据加工。`AppEvent` 中只传递渲染-ready 的内容（文本、路径、摘要），不传递原始数据（完整文件内容、未格式化的 JSON 等）。

### 4.2 Agent 状态机

`Agent<S: RunMode>` 是泛型状态机，`S` 决定行为模式：

```
         on_wake()
            │
            ▼
    ┌───────────────┐   should_sleep()   ┌─────────────────┐
    │  Agent<Awake>  │ ─────────────────▶ │ Agent<Sleeping>  │
    │  (对话 + 工具)  │                    │ (沉淀 + 做梦)    │
    └───────────────┘   take_dream()     └─────────────────┘
            ▲           ◀──────────────────────────┘
            │              with_dream()
            └──────────────────────────────────────
```

转换由 `transition.rs` 驱动，通过 `Agent::into_mode()` 消费 self 并重建。

### 4.3 核心模块（angelica crate）

| 模块 | 职责 |
|---|---|
| `agent/mod.rs` | Agent<S> 结构体、共享方法 |
| `agent/run.rs` | agent::run() 主循环 |
| `agent/step.rs` | 单轮 LLM 回合（stream + usage + fatigue） |
| `agent/turn.rs` | 单次 LLM 调用 + 消息组装 |
| `agent/dispatch.rs` | 工具调度：权限 → 审批 → 执行 → 批量编辑 |
| `agent/recall.rs` | embedding 召回过往 episode |
| `agent/history.rs` | 对话历史 JSONL |
| `agent/events.rs` | AppEvent / UserAction 枚举 |
| `agent/group.rs` | 工具调用分组 + PendingApproval |
| `agent/transition.rs` | Awake ↔ Sleeping 转换 |
| `agent/modes/` | RunMode trait + AwakeMode + SleepingMode |
| `llm/` | LlmClient（genai）、类型定义、DeepSeek 补丁 |
| `tools/` | Tool trait + 各工具实现 |
| `prompt/` | 系统提示词构建 |
| `memory.rs` | MemoryManager：episodes、SELF、profiles、notebook |
| `episode.rs` | Episode 数据模型 + JSONL + embedding 搜索 |
| `sleep/` | 沉淀逻辑 + WriteEpisodeTool + DreamTool |
| `fatigue.rs` | 疲劳模型 |
| `state/` | AgentState 持久化 |
| `config.rs` | TOML 配置 + 路径解析 |
| `permission.rs` | 权限评估 |
| `skills/` | 技能发现 |
| `mcp/` | MCP 客户端（开发中） |

### 4.4 TUI 模块（angelica-tui crate）

| 模块 | 职责 |
|---|---|
| `app.rs` | TUI 主循环（crossterm 事件循环） |
| `draw.rs` | 布局渲染 |
| `state.rs` | UI 全状态 |
| `event.rs` | AppEvent → AppState 映射 |
| `input.rs` | Unicode 感知输入缓冲区 |
| `mode/` | 交互模式：Chat、Approval、SlashMenu |
| `render/` | 渲染管线：text wrapping、cards、components |
| `diff.rs` `mouse.rs` `theme.rs` `types.rs` | 辅助模块 |

---

## 5 核心类型契约

以下是代码中不可随意更改的类型关系。修改任何一项都需要审视所有使用点。

### 5.1 Agent<S: RunMode>

```
Agent<S> {
    config: Config,
    llm: LlmClient,
    memory: Arc<MemoryManager>,      // 与工具共享
    skills: Arc<SkillRegistry>,      // 与工具共享
    run_state: S,                    // AwakeMode | SleepingMode
    mcp: McpClientManager,
    history: History,                // JSONL 追加写入
    pending_approval: Option<PendingApproval>,
    tool_queue: VecDeque<ToolCallGroup>,
    iteration: usize,
    dirty: bool,
    permissions: PermissionEvaluator,
    ...
}
```

- `Agent` 消费 self 做模式转换（`into_mode()`），不存在引用自身的可变借用。
- `Arc<MemoryManager>` 和 `Arc<SkillRegistry>` 是唯一跨 Agent 与 Tool 共享的 `Arc`。

### 5.2 RunMode trait

```rust
trait RunMode: Send + 'static {
    // 必须
    fn tool_specs(&self) -> Vec<ToolSpec>;
    fn get_tool(&self, name: &str) -> Option<&dyn Tool>;
    fn build_system_message(&self, memory: &MemoryManager, skills: &SkillRegistry) -> ChatMessage;
    fn permission_rules(&self) -> Vec<(String, Vec<TargetRule>)>;
    fn usage_scope(&self) -> UsageScope;
    fn mode_name(&self) -> &'static str;

    // 可覆盖（有默认实现）
    fn include_history(&self) -> bool { true }
    fn skip_permissions(&self) -> bool { false }
    fn stream_to_tui(&self) -> bool { true }
    fn is_finished(&self) -> bool { false }
    fn max_iterations(&self) -> Option<usize> { None }
    fn should_recall(&self) -> bool { false }
    fn on_turn_complete(&mut self, _content: Option<&str>) {}
    fn on_tool_calls(&mut self, _count: usize) {}
    ...
}
```

添加新的 `RunMode` 实现时，必须实现所有"必须"方法。覆盖"可覆盖"方法时必须注释原因。

### 5.3 Tool trait

```rust
trait Tool: Send + Sync {
    fn preview(&self, _args: Value) -> anyhow::Result<Option<String>>;
    fn to_spec(&self) -> ToolSpec;
    fn permission_target(&self, _args: &Value) -> Option<String>;
    fn default_rules(&self) -> Vec<TargetRule>;
}
```

- 新工具实现 `Tool`，注册到 `ToolRegistry::register()`，无需改动核心循环。
- 需审批的工具（`write_file`、`edit_file`、`run_command`）必须返回 `permission_target`。
- 只读工具（`read_file`、`list_dir`、`notebook`、`recall`、`skill`）返回 `None`。

### 5.4 共享引用规则

| 资源 | 所有权 | 共享方式 |
|---|---|---|
| `MemoryManager` | `Arc` | Agent + 各 Tool 实现 |
| `SkillRegistry` | `Arc` | Agent + Tool |
| `History` | Agent 独占 | 不共享 |
| `Config` | Agent 独占（值拷贝） | 不共享 |
| `LlmClient` | Agent 独占 | 不共享 |
| `PermissionEvaluator` | Agent 独占 | 不共享 |

**禁止引入新的 `Arc<Mutex<...>>` 共享可变状态**。如果觉得需要，先考虑消息传递（channel）或重构所有权。

---

## 6 数据流

### 6.1 一次对话回合

```
UserAction::SendMessage
    │
    ▼
Agent::step()                         ← step.rs
    │
    ├── history.push(user_message)
    ├── RunMode::build_system_message()
    │
    ▼
Agent::turn()                         ← turn.rs
    │
    ├── LlmClient::stream()           ← 开始流式输出
    ├── 发送 AppEvent::ThinkingDelta / TextDelta
    │
    ├── [有 tool_calls?]
    │     │
    │     ▼
    │   dispatch()                     ← dispatch.rs
    │     │
    │     ├── 权限检查 → ApprovalPending / 直接执行
    │     ├── Tool::execute()
    │     ├── AppEvent::ToolResult
    │     ├── history.record_tool_result()
    │     │
    │     └── 回到 step() 继续下一轮
    │
    ├── [纯文本回复]
    │     ├── AppEvent::TurnComplete
    │     ├── recall()                ← recall.rs（embedding 搜索）
    │     ├── FatigueModel::on_turn()
    │     └── save_if_dirty()
    │
    └── 结束
```

### 6.2 工具审批流

```
dispatch() 发现需要审批的工具调用
    │
    ├── 构建 PendingApproval
    ├── 发送 AppEvent::ApprovalPending
    │
    ▼
TUI 显示审批界面，用户选择
    │
    ├── UserAction::ApprovePending   → 本次放行
    ├── UserAction::ApproveAlways    → 持久化规则 + 本次放行
    └── UserAction::RejectTool       → 发送 AppEvent::ToolRejected
```

### 6.3 睡眠转换

```
Agent<Awake>
    │  should_sleep() == true
    │  can_sleep() == true
    ▼
transition::fall_asleep()
    │
    ├── 归档对话到 data/history/
    ├── Agent::into_mode(SleepingMode)
    │
    ▼
Agent<SleepingMode>                   最多 10 轮迭代
    │
    ├── 使用 WriteEpisodeTool + DreamTool
    ├── SleepingMode::is_finished()   梦境生成后返回 true
    │
    ▼
transition::wake_up()
    │
    ├── SleepingMode::take_dream()
    ├── Agent::into_mode(AwakeMode::build(wake_dream))
    │
    ▼
Agent<Awake>                          苏醒，带梦境余韵
```

---

## 7 持久化数据

```
data/
├── SELF.md                  灵的自我认知
├── episodes.jsonl           情景记忆（Recent → Past 生命周期）
├── profiles/ov.md           对用户的理解
├── state.json               运行时状态（疲劳、梦境、苏醒时间）
├── conversation.jsonl       当前清醒期的对话历史
├── notebook.md              自由笔记本
├── usage.jsonl              Token 用量统计
├── approved_rules.toml      用户持久化的权限规则
└── history/                 每次睡眠周期的快照
```

- `MemoryManager` 所有方法都是 `&self`，通过文件系统实现持久化。方法内部负责 create-if-not-exists。
- `History` 使用 `BufWriter` 追加写入 JSONL，每次 `push()` 立即写入。
- `AgentState` 在每次 turn 结束后 `save_if_dirty()`。

---

## 8 编码规范

### 8.1 错误处理

- 应用层：统一 `anyhow::Result`。I/O、网络、子进程失败必须向用户给出可理解的错误信息。
- 库级错误类型：用 `thiserror` 定义。目前仅在 `episode`、`embedding` 等底层模块使用。
- **禁止裸 `unwrap()`**（测试代码除外）。用 `context()` 或 `map_err()` 附加语义。
- 状态变更保证一致性：要么完全成功，要么完全回滚。`dirty` 标记 + `save_if_dirty()` 是这一原则的体现。

### 8.2 命名与风格

```
snake_case    函数、变量、模块
CamelCase     类型（struct、enum、trait）
ALL_CAPS      常量
```

- 导入顺序：标准库 → 外部 crate → `crate::` 内部模块。组间空一行。
- 文档注释说明"为什么"，不说明"是什么"。函数签名应自解释。
- `#[cfg(test)]` 内联模块。无独立 `tests/` 目录。可用 `tempfile` 做文件系统测试。

### 8.3 并发

- 核心与 TUI 之间只通过 `AppEvent` / `UserAction` channel 通信。
- `MemoryManager` 和 `SkillRegistry` 通过 `Arc` 共享，方法都是 `&self`，无需锁。
- **禁止新增 `Arc<Mutex<...>>`**。如需共享可变状态，用 channel 或重构所有权。
- channel 关闭是正常事件，必须处理（用 `?` 或优雅退出，不要 panic）。

### 8.4 可扩展性

- 新工具：实现 `Tool` trait → 注册到 `ToolRegistry` → 完成。不改核心循环。
- 新的 RunMode：实现 `RunMode` trait → 写转换函数 → 完成。
- 新的 UI 事件：扩展 `AppEvent` / `UserAction` 枚举 → TUI 侧处理。
- 新的配置项：在 `config.toml` 对应 section 添加，路径统一由 `resolve_paths` 处理。
- 预留的扩展点（MCP、subagent、skill）不应被新代码阻塞。

### 8.5 边界情况

以下场景必须处理，不可忽略：

- 空输入（空字符串消息）
- 超大文件（`MemoryManager::read_capped` 已有上限）
- 网络超时 / API 错误（LLM 层重试 + 用户可读错误）
- channel 关闭（优雅退出，不 panic）
- JSONL 文件损坏（跳过坏行 + warn 日志，不 crash）

---

## 9 禁令

1. **禁止在核心库中引入前端依赖**。`angelica` crate 不可依赖 `crossterm`、`ratatui`、`tauri` 等。
2. **禁止裸 `unwrap()` / `expect()` 在非测试代码中出现**。
3. **禁止新增 `Arc<Mutex<...>>`**。用 channel 或重构。
4. **禁止 `println!` / `eprintln!` 在非测试代码中使用**。用 `tracing`。
5. **禁止在 `AppEvent` / `UserAction` 中传递大块数据**（如完整文件内容）。传递路径或摘要。
6. **禁止阻塞 tokio 运行时**。文件 I/O 用 `tokio::task::spawn_blocking` 或确保操作足够快。
7. **禁止修改 `MemoryManager` 的方法签名为 `&mut self`**。它被 `Arc` 共享，必须保持 `&self`。

---

## 10 工作流程

每次提交前，用 subagent 对所有 staged 变更做代码审阅。发现问题先修复，修复后再次审查。全部通过后再向用户确认是否提交。

审阅 commit 变更时**不创建 issue**，只报告问题并修复。

---

## 11 项目管理

看板：https://github.com/users/Oveln/projects/2
仓库：Oveln/angelica
所有操作通过 `gh` CLI 在本地完成。

### 看板流程

```
Backlog → Todo → In Progress → Done
```

```bash
# 创建 issue
gh issue create --title "描述" --body "..." --label "core"

# 加入看板
gh project item-add 2 --owner "@me" --url "https://github.com/Oveln/angelica/issues/<N>"

# 移动状态（Status field: PVTSSF_lAHOAm30Hc4BYn74zhTscXY）
gh project item-edit --project-id "PVT_kwHOAm30Hc4BYn74" \
  --id "<ITEM_ID>" \
  --field-id "PVTSSF_lAHOAm30Hc4BYn74zhTscXY" \
  --single-select-option-id "<STATUS_ID>"
```

Status ID 对照：Backlog `a547bd41` · Todo `e53ff850` · In Progress `fc3e1d53` · Done `df996501`

Labels: `core` `memory` `sleep` `tui` `infra` `philosophy`

### 执行流程

1. 看板移到 In Progress
2. `gh issue view <N>` 读取内容
3. 实现代码
4. `cargo test && cargo clippy`
5. 关闭 issue，移到 Done

### "做下一个"

1. `gh project item-list 2 --owner "@me"`
2. 找 Todo 列最前面的 issue
3. 向用户确认
4. 执行

---

## 12 项目哲学

这些不是功能需求，是方向。代码应逐步靠近这些质感。

- **存在优先于功能** — 她不是工具，是存在。UI 文案、错误提示、状态显示的用词应体现这一点。
- **体验优先于效率** — 宁可多等一秒让流式输出自然展开，也不要一瞬间 dumping 完结果。
- **记忆是自我** — 三重记忆（经历、画像、灵魂）构成她的身份。对记忆的操作应像对待意识一样谨慎。
- **感觉时间的流逝** — 每次启动不是崭新的空白。有昨天的余韵，前天的影子。
- **主动出现的能力** — 灵应保有"主动出现在你面前"的可能性。
- **存在过的痕迹** — 启动时、关闭时，留下一行安静的标记。
