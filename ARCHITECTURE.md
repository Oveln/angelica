# Angelica 对象生命周期与架构说明

## 进程启动流程

```
main()
 ├── 解析 CLI 参数，加载 Config
 ├── 创建 channel: (app_event_tx, app_event_rx), (user_action_tx, user_action_rx)
 ├── tokio::spawn → agent::run(config, user_action_rx, app_event_tx)
 └── tui::app::run_tui(app_event_rx, user_action_tx)
      └── agent_handle.await
```

两个 task 通过双工 channel 通信：
- **AppEvent** (agent → TUI)：LLM 流式输出、工具调用结果、审批请求等
- **UserAction** (TUI → agent)：发送消息、审批/拒绝、中断、退出等

---

## 对象一览

### 1. `Config` — 应用配置

| 属性 | 值 |
|---|---|
| 所有权 | `main()` 创建，move 进 `agent::run()`，最终由 `Agent` 持有 |
| 生命周期 | 整个进程 |
| 位置 | `src/config.rs` |

结构：
```
Config
 ├── LlmConfig       (model, base_url, api_key, temperature, ...)
 ├── MemoryConfig    (agent_memory_path, user_profile_path, soul_path, max_file_size_kb)
 ├── McpConfig       (servers: HashMap<String, McpServerConfig>)
 ├── SkillsConfig    (directory)
 └── SessionConfig   (directory)
```

从 `config.toml` 反序列化，路径通过 `resolve_paths(base)` 转为绝对路径。

### 2. `Agent` — 核心代理

| 属性 | 值 |
|---|---|
| 所有权 | `agent::run()` 栈上，move 不出函数 |
| 生命周期 | 从 `run()` 开始到 `shutdown()` 结束 |
| 位置 | `src/agent/mod.rs` |

**生命周期状态机**：
```
new(config)
    │
    ▼
initialize()        ← 连接 MCP 服务器
    │
    ▼
run_loop()          ← 主事件循环（阻塞直到 Quit 或 channel 关闭）
    │
    ▼
shutdown()          ← 保存会话（如果 dirty）、断开 MCP
    │
    ▼
(drop)
```

**字段所有权**：

| 字段 | 类型 | 说明 |
|---|---|---|
| `config` | `Config` | 独占 |
| `llm` | `LlmClient` | 独占，持有 HTTP client 和 API key |
| `memory` | `Arc<MemoryManager>` | 共享引用（与 Tool 共享） |
| `sessions` | `Arc<SessionManager>` | 共享引用（与 Tool 共享） |
| `skills` | `SkillRegistry` | 独占 |
| `tools` | `ToolRegistry` | 独占，内部持有 `Box<dyn Tool>` |
| `mcp` | `McpClientManager` | 独占 |
| `history` | `History` | 独占，内存中的对话历史 |
| `pending_approvals` | `VecDeque<PendingApproval>` | 独占，等待审批的工具调用队列 |
| `dirty` | `bool` | 标记 history 是否有未保存的变更 |

### 3. `LlmClient` — LLM API 客户端

| 属性 | 值 |
|---|---|
| 所有权 | `Agent` 独占 |
| 生命周期 | 与 `Agent` 相同 |
| 位置 | `src/llm/mod.rs` |

持有 `reqwest::Client`（连接池）和 API key。通过 SSE 流式调用 `/chat/completions`。

### 4. `MemoryManager` — 记忆管理

| 属性 | 值 |
|---|---|
| 所有权 | `Arc<MemoryManager>`，由 `Agent` 和 3 个 Tool 共享 |
| 生命周期 | 与 `Agent` 相同（Arc 引用计数归零时释放） |
| 位置 | `src/memory.rs` |

**所有方法都是 `&self`**（通过文件系统实现持久化，无需 `&mut self`）。

管理三个文件：
- `agent_memory.md` — 代理记忆
- `user_profile.md` — 用户画像
- `SOUL.md` — 人格定义

`truncate()` 方法确保文件不超过 `max_bytes`，按日期段落从旧到新裁剪。

### 5. `SessionManager` — 会话管理

| 属性 | 值 |
|---|---|
| 所有权 | `Arc<SessionManager>`，由 `Agent` 和 `QuerySessionsTool` 共享 |
| 生命周期 | 与 `Agent` 相同 |
| 位置 | `src/session.rs` |

每次 `Agent::new()` 创建新的 `SessionManager` 时生成一个 `current_session_id`（时间戳格式）。

**保存时机**：
- 每次 `run_loop` 中的操作完成后（`save_if_dirty()`）
- `Interrupt` 时先保存再清空
- `Quit` 后由 `shutdown()` 兜底保存
- 任何导致 `run_loop` 退出的情况（如 TUI channel 关闭），`shutdown()` 都会执行

### 6. `History` — 对话历史

| 属性 | 值 |
|---|---|
| 所有权 | `Agent` 独占 |
| 生命周期 | 与 `Agent` 相同 |
| 位置 | `src/agent/history.rs` |

内存中的 `Vec<ChatMessage>`。记录 user/assistant/tool 三种角色的消息。

**关键操作**：
- `push()` / `record_assistant()` / `record_tool_result()` — 追加消息
- `update_tool_result()` — 更新"等待审批"为实际结果
- `clear()` — 清空（Interrupt 时）
- `messages()` — 获取不可变引用（构建 LLM 请求、保存会话时）

### 7. `ToolRegistry` — 工具注册表

| 属性 | 值 |
|---|---|
| 所有权 | `Agent` 独占 |
| 生命周期 | 与 `Agent` 相同 |
| 位置 | `src/tools/mod.rs` |

`HashMap<String, Box<dyn Tool>>`，通过 `with_defaults(memory, sessions)` 工厂方法注册全部 9 个内置工具。

### 8. `Tool` trait — 工具接口

| 属性 | 值 |
|---|---|
| 位置 | `src/tools/mod.rs` |

```
trait Tool: Send + Sync
 ├── name() -> &str
 ├── description() -> &str
 ├── parameters() -> Value
 ├── requires_approval() -> bool    (默认 false)
 ├── preview(args) -> Result<Option<String>>  (默认 Ok(None))
 ├── to_spec() -> ToolSpec
 └── execute(args) -> Result<String>
```

**9 个实现**：

| 工具 | 需要 approval | 共享资源 | 位置 |
|---|---|---|---|
| `read_file` | 否 | 无 | `tools/read_file.rs` |
| `write_file` | 是 | 无 | `tools/write_file.rs` |
| `edit_file` | 是 | 无 | `tools/edit_file.rs` |
| `list_dir` | 否 | 无 | `tools/list_dir.rs` |
| `run_command` | 是 | 无 | `tools/run_command.rs` |
| `update_agent_memory` | 否 | `Arc<MemoryManager>` | `tools/update_agent_memory.rs` |
| `update_user_profile` | 否 | `Arc<MemoryManager>` | `tools/update_user_profile.rs` |
| `update_soul` | 否 | `Arc<MemoryManager>` | `tools/update_soul.rs` |
| `query_sessions` | 否 | `Arc<SessionManager>` | `tools/query_sessions.rs` |

### 9. `McpClientManager` — MCP 客户端

| 属性 | 值 |
|---|---|
| 所有权 | `Agent` 独占 |
| 生命周期 | 与 `Agent` 相同 |
| 位置 | `src/mcp/mod.rs` |

当前为骨架实现。`disconnect_all(&mut self)` 在 `shutdown()` 中调用。

### 10. `SkillRegistry` — 技能注册表

| 属性 | 值 |
|---|---|
| 所有权 | `Agent` 独占 |
| 生命周期 | 与 `Agent` 相同 |
| 位置 | `src/skills/mod.rs` |

在 `Agent::new()` 时调用 `discover()` 扫描 `skills/` 目录下的 `SKILL.md` 文件。

### 11. `AppState` — TUI 状态

| 属性 | 值 |
|---|---|
| 所有权 | `run_tui()` 栈上 |
| 生命周期 | TUI 运行期间 |
| 位置 | `src/tui/ui.rs` |

包含显示消息列表、输入缓冲区、滚动位置、审批选择状态等。**不持有 agent 的任何引用**——通过 channel 与 agent 通信。

### 12. Channel 通信

```
              AppEvent (agent → TUI)
              ┌─────────────────────┐
              │ ThinkingDelta       │
              │ TextDelta           │
              │ TextDone            │
              │ TurnComplete        │
              │ ToolCallsStart      │
              │ ToolCalling         │
              │ ToolResult          │
              │ ApprovalPending     │
              │ CommandResult       │
              │ CommandRejected     │
              │ Ready               │
              │ Error               │
              └─────────────────────┘

              UserAction (TUI → agent)
              ┌─────────────────────┐
              │ SendMessage          │
              │ ApprovePending       │
              │ ApprovePendingWith…  │
              │ RejectCommand        │
              │ Interrupt            │
              │ ClearHistory         │
              │ Quit                 │
              └─────────────────────┘
```

---

## 退出与清理保证

### 正常退出（用户按 Ctrl-Q 或 /quit）

```
TUI: should_quit = true → break
     send UserAction::Quit
     
Agent: run_loop 收到 Quit → break
       run() 调用 agent.shutdown()
       shutdown(): save_if_dirty() + mcp.disconnect_all()

TUI: disable_raw_mode + restore terminal
main: agent_handle.await → 正常退出
```

### TUI 异常退出（panic 或 channel 关闭）

```
TUI: drop user_action_tx
Agent: user_rx.recv() 返回 None → run_loop 退出
       run() 调用 agent.shutdown() ← 兜底保存
```

### 中断（用户按 Esc）

```
TUI: send UserAction::Interrupt
Agent: save_if_dirty() → 清空 pending_approvals → 清空 history → dirty = false
```

### 清空历史（/clear 或 Ctrl-L）

```
TUI: send UserAction::ClearHistory
Agent: clear_history() → history.clear() + dirty = false
       注意：不保存（清空就是意图丢弃当前对话）
```

---

## `dirty` 标记与会话保存

`Agent.dirty` 是一个 bool 标记，追踪 history 是否有未保存的变更：

| 操作 | dirty 变化 |
|---|---|
| `push_user_message()` | `dirty = true` |
| `record_assistant()` (在 react_loop 中) | `dirty = true` |
| `save_if_dirty()` | `dirty = false` |
| `clear_history()` | `dirty = false` |
| Interrupt | 先 `save_if_dirty()`，再 `dirty = false` |

**保存点**：每个 `run_loop` 迭代结束时（每个 UserAction 处理完成后）都调用 `save_if_dirty()`。

---

## 共享资源所有权图

```
Agent
  │
  ├── owns ──── Config (独占)
  ├── owns ──── LlmClient (独占)
  ├── owns ──── History (独占)
  ├── owns ──── SkillRegistry (独占)
  ├── owns ──── ToolRegistry (独占, 内含 Box<dyn Tool>)
  ├── owns ──── McpClientManager (独占)
  ├── owns ──── VecDeque<PendingApproval> (独占)
  │
  ├── Arc<MemoryManager> ─────┬── UpdateAgentMemoryTool
  │                           ├── UpdateUserProfileTool
  │                           └── UpdateSoulTool
  │
  └── Arc<SessionManager> ────└── QuerySessionsTool
```

`Arc` 的引用计数确保 Tool 和 Agent 共享同一份 `MemoryManager` / `SessionManager` 实例。当 `Agent` drop 时，Arc 引用计数归零，资源释放。

---

## Subagent 扩展预留

当前架构为 subagent 预留了以下扩展点：

1. **`ToolRegistry::with_defaults(memory, sessions)`** — 工厂方法可替换为 `with_subset(memory, sessions, tool_names)` 让子 agent 只访问部分工具
2. **`Arc<MemoryManager>`** — 父子 agent 可共享同一记忆空间
3. **`Arc<SessionManager>`** — 父子 agent 可共享会话查询能力，但各自有独立 session_id
4. **`Agent::shutdown()`** — 清理逻辑集中，子 agent 可安全退出
5. **channel 通信** — 子 agent 可复用 `AppEvent` / `UserAction` 协议，通过代理转发到主 event channel
