use axum::Json;
use axum::Router;
use axum::extract::State;
use axum::response::Html;
use axum::routing::get;
use std::sync::Arc;
use tokio::sync::watch;
use tower_http::cors::CorsLayer;

use super::snapshot::DebugSnapshot;

// ── Shared state ──────────────────────────────────────────────

#[derive(Clone)]
pub struct DebugState {
    pub snapshot_rx: watch::Receiver<DebugSnapshot>,
}

// ── Server startup ────────────────────────────────────────────

pub fn start_debug_server(addr: std::net::SocketAddr, snapshot_rx: watch::Receiver<DebugSnapshot>) {
    let state = DebugState { snapshot_rx };
    let app = Router::new()
        .route("/", get(handler_index))
        .route("/context", get(handler_context))
        .route("/context.txt", get(handler_context_text))
        .route("/api/snapshot.json", get(handler_snapshot_json))
        .route("/state", get(handler_state))
        .route("/episodes", get(handler_episodes))
        .layer(CorsLayer::permissive())
        .with_state(Arc::new(state));

    std::thread::spawn(move || {
        let rt = tokio::runtime::Runtime::new().expect("debug server runtime");
        rt.block_on(async move {
            let listener = match tokio::net::TcpListener::bind(addr).await {
                Ok(l) => l,
                Err(e) => {
                    tracing::warn!("Debug server bind failed: {e}");
                    return;
                }
            };
            if let Err(e) = axum::serve(listener, app).await {
                tracing::warn!("Debug server error: {e}");
            }
        });
    });
}

// ── Helpers ───────────────────────────────────────────────────

fn borrow_snapshot(state: &DebugState) -> DebugSnapshot {
    state.snapshot_rx.borrow().clone()
}

fn style_css() -> &'static str {
    r#"
    :root {
      --bg: #0a0a0a;
      --surface: #141414;
      --surface2: #1c1c1c;
      --border: #2a2a2a;
      --text: #e0e0e0;
      --text-dim: #888;
      --accent: #7c9eff;
      --accent-dim: #4a6a9e;
      --success: #6ee7a0;
      --warn: #f5c542;
      --error: #ef6b6b;
    }
    * { margin: 0; padding: 0; box-sizing: border-box; }
    body {
      font-family: 'SF Mono', 'JetBrains Mono', 'Menlo', monospace;
      background: var(--bg);
      color: var(--text);
      font-size: 13px;
      line-height: 1.5;
      padding: 24px;
    }
    a { color: var(--accent); text-decoration: none; }
    a:hover { text-decoration: underline; }
    nav { margin-bottom: 24px; display: flex; gap: 16px; align-items: center; }
    nav a { font-size: 13px; }
    h1 { font-size: 16px; font-weight: 600; color: var(--text); margin-bottom: 16px; }
    h2 { font-size: 14px; font-weight: 600; color: var(--text-dim); margin: 20px 0 8px; }
    .grid {
      display: grid;
      grid-template-columns: repeat(auto-fill, minmax(180px, 1fr));
      gap: 12px;
      margin-bottom: 20px;
    }
    .card {
      background: var(--surface);
      border: 1px solid var(--border);
      border-radius: 6px;
      padding: 12px 14px;
    }
    .card .label { font-size: 11px; color: var(--text-dim); text-transform: uppercase; letter-spacing: 0.5px; }
    .card .value { font-size: 20px; font-weight: 600; color: var(--accent); margin-top: 2px; }
    .card .sub { font-size: 11px; color: var(--text-dim); margin-top: 2px; }
    .bar-track { background: var(--surface2); border-radius: 3px; height: 6px; margin-top: 6px; }
    .bar-fill { height: 100%; border-radius: 3px; transition: width 0.3s; }
    .msg {
      background: var(--surface);
      border: 1px solid var(--border);
      border-radius: 6px;
      margin-bottom: 8px;
      overflow: hidden;
    }
    .msg-header {
      display: flex;
      justify-content: space-between;
      align-items: center;
      padding: 8px 14px;
      cursor: pointer;
      user-select: none;
    }
    .msg-header:hover { background: var(--surface2); }
    .msg-role {
      font-weight: 600;
      font-size: 12px;
    }
    .msg-role.system { color: var(--accent); }
    .msg-role.user { color: var(--success); }
    .msg-role.assistant { color: var(--warn); }
    .msg-role.tool { color: var(--text-dim); }
    .msg-meta { font-size: 11px; color: var(--text-dim); }
    .msg-body {
      display: none;
      padding: 10px 14px;
      border-top: 1px solid var(--border);
      white-space: pre-wrap;
      word-break: break-all;
      font-size: 12px;
      max-height: 60vh;
      overflow-y: auto;
    }
    .msg-body.open { display: block; }
    details { margin-bottom: 6px; }
    details summary {
      cursor: pointer;
      padding: 6px 0;
      color: var(--text-dim);
      font-size: 12px;
    }
    details summary:hover { color: var(--text); }
    .raw { white-space: pre-wrap; word-break: break-all; font-size: 12px; }
  "#
}

fn nav_html() -> String {
    r#"<nav>
      <a href="/">angelica debug</a>
      <a href="/context">context</a>
      <a href="/context.txt">raw</a>
      <a href="/state">state</a>
      <a href="/episodes">episodes</a>
    </nav>"#
        .to_string()
}

fn fatigue_bar(fatigue: f64) -> String {
    let pct = (fatigue * 100.0).min(100.0) as usize;
    let color = if fatigue < 0.3 {
        "var(--success)"
    } else if fatigue < 0.6 {
        "var(--warn)"
    } else {
        "var(--error)"
    };
    format!(
        r#"<div class="bar-track"><div class="bar-fill" style="width:{pct}%;background:{color}"></div></div>"#
    )
}

fn auto_refresh_html(secs: u32) -> String {
    format!(r#"<meta http-equiv="refresh" content="{secs}">"#)
}

// ── Handlers ──────────────────────────────────────────────────

async fn handler_index(State(state): State<Arc<DebugState>>) -> Html<String> {
    let snap = borrow_snapshot(&state);
    let total_chars = snap.total_context_chars();
    let total_est_tokens = total_chars / 4; // rough estimate

    Html(format!(
        r##"<!DOCTYPE html>
<html><head>
  <meta charset="utf-8">
  {refresh}
  <style>{css}</style>
  <title>angelica debug</title>
</head><body>
{nav}
<h1>overview</h1>
<div class="grid">
  <div class="card">
    <div class="label">mode</div>
    <div class="value">{mode}</div>
  </div>
  <div class="card">
    <div class="label">messages</div>
    <div class="value">{msgs}</div>
    <div class="sub">{total_chars} chars ~{total_est_tokens} tokens</div>
  </div>
  <div class="card">
    <div class="label">iteration</div>
    <div class="value">{iter}</div>
    <div class="sub">queue: {queue}</div>
  </div>
  <div class="card">
    <div class="label">fatigue</div>
    <div class="value">{fatigue_pct}%</div>
    <div class="sub">{fatigue_desc}</div>
    {fatigue_bar}
  </div>
  <div class="card">
    <div class="label">turns</div>
    <div class="value">{turns}</div>
    <div class="sub">tool calls: {tool_calls}</div>
  </div>
  <div class="card">
    <div class="label">tools</div>
    <div class="value">{tool_count}</div>
  </div>
  <div class="card">
    <div class="label">recall</div>
    <div class="value">{recall:.2}</div>
    <div class="sub" style="max-width:180px;overflow:hidden;text-overflow:ellipsis;white-space:nowrap">{recall_preview}</div>
  </div>
  <div class="card">
    <div class="label">last tokens</div>
    <div class="value">{prompt_tok} / {comp_tok}</div>
    <div class="sub">prompt / completion</div>
  </div>
</div>
</body></html>"##,
        refresh = auto_refresh_html(3),
        css = style_css(),
        nav = nav_html(),
        mode = snap.mode,
        msgs = snap.history_messages,
        iter = snap.iteration,
        queue = snap.tool_queue_len,
        fatigue_pct = (snap.fatigue * 100.0).round() as usize,
        fatigue_desc = snap.fatigue_desc,
        fatigue_bar = fatigue_bar(snap.fatigue),
        turns = snap.turns,
        tool_calls = snap.tool_calls,
        tool_count = snap.tool_count,
        recall = snap.recall_top_score,
        recall_preview = html_escape(&snap.recall_text_preview),
        prompt_tok = snap
            .last_prompt_tokens
            .map(|v| v.to_string())
            .unwrap_or("-".into()),
        comp_tok = snap
            .last_completion_tokens
            .map(|v| v.to_string())
            .unwrap_or("-".into()),
        total_chars = total_chars,
        total_est_tokens = total_est_tokens,
    ))
}

async fn handler_context(State(state): State<Arc<DebugState>>) -> Html<String> {
    let snap = borrow_snapshot(&state);

    let mut messages_html = String::new();
    for (i, msg) in snap.context_messages.iter().enumerate() {
        let role_class = match msg.role.as_str() {
            "system" => "system",
            "user" => "user",
            "assistant" => "assistant",
            "tool" => "tool",
            _ => "",
        };
        let name_tag = msg
            .name
            .as_ref()
            .map(|n| format!(" <span style=\"color:var(--text-dim)\">({n})</span>"))
            .unwrap_or_default();
        let tc_tag = msg
            .tool_calls_count
            .map(|c| format!(" | {c} tool calls"))
            .unwrap_or_default();
        let tc_id_tag = msg
            .tool_call_id
            .as_ref()
            .map(|id| format!(" | id: {id}"))
            .unwrap_or_default();

        messages_html.push_str(&format!(
            r#"<div class="msg">
  <div class="msg-header" onclick="toggle({i})">
    <span class="msg-role {role_class}">{role}{name_tag}</span>
    <span class="msg-meta">{len} chars{tc_tag}{tc_id_tag}</span>
  </div>
  <div class="msg-body" id="msg-{i}">{content}</div>
</div>"#,
            role = msg.role,
            len = msg.content_length,
            content = html_escape(&msg.content_preview),
        ));
    }

    Html(format!(
        r#"<!DOCTYPE html>
<html><head>
  <meta charset="utf-8">
  {refresh}
  <style>{css}</style>
  <title>context — angelica debug</title>
</head><body>
{nav}
<h1>context ({count} messages, {chars} chars)</h1>
{messages}
<script>
function toggle(i) {{
  var el = document.getElementById('msg-' + i);
  el.classList.toggle('open');
}}
</script>
</body></html>"#,
        refresh = auto_refresh_html(5),
        css = style_css(),
        nav = nav_html(),
        count = snap.context_messages.len(),
        chars = snap.total_context_chars(),
        messages = messages_html,
    ))
}

async fn handler_context_text(State(state): State<Arc<DebugState>>) -> String {
    let snap = borrow_snapshot(&state);
    let mut out = String::new();
    for msg in &snap.context_messages {
        out.push_str(&format!(
            "═══ {} ({} chars) ═══\n{}\n\n",
            msg.role, msg.content_length, msg.content_preview
        ));
    }
    out
}

async fn handler_state(State(state): State<Arc<DebugState>>) -> Html<String> {
    let snap = borrow_snapshot(&state);
    let json = serde_json::to_string_pretty(&snap).unwrap_or_default();

    Html(format!(
        r#"<!DOCTYPE html>
<html><head>
  <meta charset="utf-8">
  <style>{css}</style>
  <title>state — angelica debug</title>
</head><body>
{nav}
<h1>state</h1>
<div class="raw">{json}</div>
</body></html>"#,
        css = style_css(),
        nav = nav_html(),
        json = html_escape(&json),
    ))
}

async fn handler_episodes(State(state): State<Arc<DebugState>>) -> Html<String> {
    let snap = borrow_snapshot(&state);

    Html(format!(
        r#"<!DOCTYPE html>
<html><head>
  <meta charset="utf-8">
  <style>{css}</style>
  <title>episodes — angelica debug</title>
</head><body>
{nav}
<h1>episodes</h1>
<p style="color:var(--text-dim)">Episode data is loaded from data/episodes.jsonl at agent startup.</p>
<p>Current snapshot contains {msgs} context messages. Full episode list will be available once integrated.</p>
</body></html>"#,
        css = style_css(),
        nav = nav_html(),
        msgs = snap.history_messages,
    ))
}

async fn handler_snapshot_json(State(state): State<Arc<DebugState>>) -> Json<DebugSnapshot> {
    Json(borrow_snapshot(&state))
}

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}
