<script lang="ts">
  import type { ApprovalPending as ApprovalPayload } from '$lib/api';
  import { renderDiff, isDiffContent } from '$lib/diff';

  let {
    approval,
    onApprove,
    onApproveAlwaysSession,
    onApproveAlwaysPersist,
    onReject,
  }: {
    approval: ApprovalPayload;
    onApprove: () => void;
    onApproveAlwaysSession: () => void;
    onApproveAlwaysPersist: () => void;
    onReject: (feedback?: string) => void;
  } = $props();

  const toolName = approval.tool_name;
  const toolTarget = approval.tool_target;
  const preview = approval.preview;
  const diffHtml = isDiffContent(preview) ? renderDiff(preview) : '';
  const isDiff = diffHtml !== '';

  let rejectFeedback = $state('');
  let showRejectInput = $state(false);
  let selected = $state(0);
  const choices = ['allow', 'always-session', 'always-persist', 'reject', 'feedback'] as const;

  function handleKeydown(e: KeyboardEvent) {
    if (isEditable(e.target)) return;

    if (showRejectInput) {
      if (e.key === 'Escape') {
        showRejectInput = false;
        selected = 3;
        e.preventDefault();
      }
      if (e.key === 'Enter' && !e.shiftKey) {
        e.preventDefault();
        onReject(rejectFeedback || undefined);
      }
      return;
    }

    switch (e.key) {
      case 'ArrowRight':
      case 'Tab':
        e.preventDefault();
        selected = (selected + 1) % choices.length;
        break;
      case 'ArrowLeft':
        e.preventDefault();
        selected = (selected - 1 + choices.length) % choices.length;
        break;
      case 'Enter':
        e.preventDefault();
        executeSelected();
        break;
      case 'y':
        e.preventDefault();
        onApprove();
        break;
      case 'a':
        e.preventDefault();
        onApproveAlwaysSession();
        break;
      case 'p':
        e.preventDefault();
        onApproveAlwaysPersist();
        break;
      case 'n':
        e.preventDefault();
        onReject();
        break;
      case 'e':
        e.preventDefault();
        selected = 4;
        showRejectInput = true;
        break;
    }
  }

  function executeSelected() {
    switch (choices[selected]) {
      case 'allow': onApprove(); break;
      case 'always-session': onApproveAlwaysSession(); break;
      case 'always-persist': onApproveAlwaysPersist(); break;
      case 'reject': onReject(); break;
      case 'feedback': showRejectInput = true; break;
    }
  }

  function selectAndClick(idx: number) {
    selected = idx;
    executeSelected();
  }

  function isEditable(target: EventTarget | null): boolean {
    if (!target) return false;
    const el = target as HTMLElement;
    const tag = el.tagName;
    return tag === 'INPUT' || tag === 'TEXTAREA' || tag === 'SELECT' || el.isContentEditable;
  }
</script>

<svelte:window onkeydown={handleKeydown} />

<div class="approval-panel">
  <div class="approval-content">
    <div class="approval-header">
      <span class="approval-icon">&#9651;</span>
      <span class="approval-label">需要审批</span>
      <span class="approval-tool">
        &middot; {toolName}
        {#if toolTarget}
          <span class="approval-target">&rarr; {toolTarget}</span>
        {/if}
      </span>
    </div>

    {#if isDiff}
      <pre class="approval-preview diff-preview">{@html diffHtml}</pre>
    {:else}
      <pre class="approval-preview">{preview}</pre>
    {/if}

    {#if showRejectInput}
      <div class="feedback-area">
        <input
          type="text"
          class="feedback-input"
          placeholder="拒绝原因（可选），Enter 确认"
          bind:value={rejectFeedback}
        />
        <div class="feedback-hint">
          <button class="hint-btn" onclick={() => onReject(rejectFeedback || undefined)}>确认</button>
          <button class="hint-btn" onclick={() => { showRejectInput = false; selected = 3; }}>返回</button>
        </div>
      </div>
    {:else}
      <div class="choices">
        <button
          class="choice-btn {selected === 0 ? 'selected' : ''} choice-allow"
          onclick={() => selectAndClick(0)}
        >
          允许 <span class="key-hint">[y]</span>
        </button>
        <button
          class="choice-btn {selected === 1 ? 'selected' : ''} choice-always"
          onclick={() => selectAndClick(1)}
        >
          始终允许 <span class="key-hint">[a]</span>
        </button>
        <button
          class="choice-btn {selected === 2 ? 'selected' : ''} choice-persist"
          onclick={() => selectAndClick(2)}
        >
          永久允许 <span class="key-hint">[p]</span>
        </button>
        <button
          class="choice-btn {selected === 3 ? 'selected' : ''} choice-reject"
          onclick={() => selectAndClick(3)}
        >
          拒绝 <span class="key-hint">[n]</span>
        </button>
        <button
          class="choice-btn {selected === 4 ? 'selected' : ''} choice-feedback"
          onclick={() => { selected = 4; showRejectInput = true; }}
        >
          反馈 <span class="key-hint">[e]</span>
        </button>
      </div>
      <div class="nav-hint">
        &larr;&rarr; 选择 &middot; Tab 下一个 &middot; y/a/p/n 快捷键
      </div>
    {/if}
  </div>
</div>

<style>
  .approval-panel {
    border-top: 1px solid rgba(200, 168, 130, 0.15);
    padding: 12px 24px;
    animation: slide-up 0.3s ease-out;
  }

  @keyframes slide-up {
    from { opacity: 0; transform: translateY(8px); }
    to { opacity: 1; transform: translateY(0); }
  }

  .approval-content {
    max-width: 640px;
    margin: 0 auto;
  }

  .approval-header {
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: 0.8125rem;
    letter-spacing: 0.05em;
  }

  .approval-icon {
    color: var(--color-amber);
  }

  .approval-label {
    color: var(--color-amber);
    font-weight: 500;
  }

  .approval-tool {
    color: var(--color-ink-light);
    font-family: var(--font-mono);
    font-size: 0.75rem;
  }

  .approval-target {
    color: var(--color-ink-faint);
  }

  .approval-preview {
    margin-top: 8px;
    max-height: 240px;
    overflow: auto;
    white-space: pre-wrap;
    font-size: 0.75rem;
    color: var(--color-ink-faint);
    font-family: var(--font-mono);
    line-height: 1.5;
  }

  .diff-preview :global(.diff-add) {
    color: #6ec87a;
  }

  .diff-preview :global(.diff-del) {
    color: #e06070;
  }

  .diff-preview :global(.diff-hunk) {
    color: #6cb4d9;
  }

  .diff-preview :global(.diff-header) {
    color: var(--color-ink-dark);
  }

  .diff-preview :global(.diff-ctx) {
    color: var(--color-ink-faint);
  }

  .choices {
    display: flex;
    align-items: center;
    gap: 6px;
    margin-top: 12px;
    flex-wrap: wrap;
  }

  .choice-btn {
    font-family: var(--font-serif);
    font-size: 0.75rem;
    letter-spacing: 0.08em;
    padding: 5px 12px;
    border-radius: 4px;
    border: 1px solid transparent;
    cursor: pointer;
    transition: all 0.2s;
    background: transparent;
    color: var(--color-ink-faint);
  }

  .choice-btn:hover {
    opacity: 0.85;
  }

  .choice-btn.selected {
    color: #000;
    font-weight: 500;
  }

  .choice-btn.selected.choice-allow {
    background: #5a9e6f;
    border-color: #5a9e6f;
  }
  .choice-btn.selected.choice-always {
    background: #5a8fb4;
    border-color: #5a8fb4;
  }
  .choice-btn.selected.choice-persist {
    background: #8a6fb4;
    border-color: #8a6fb4;
  }
  .choice-btn.selected.choice-reject {
    background: #b45a5a;
    border-color: #b45a5a;
  }
  .choice-btn.selected.choice-feedback {
    background: #b49a5a;
    border-color: #b49a5a;
  }

  .choice-btn:not(.selected).choice-allow { color: #5a9e6f; }
  .choice-btn:not(.selected).choice-reject { color: #b45a5a; }

  .key-hint {
    font-family: var(--font-mono);
    font-size: 0.65rem;
    opacity: 0.6;
    margin-left: 2px;
  }

  .nav-hint {
    margin-top: 8px;
    font-size: 0.65rem;
    color: var(--color-ink-dark);
    letter-spacing: 0.05em;
  }

  .feedback-area {
    margin-top: 12px;
  }

  .feedback-input {
    width: 100%;
    background: transparent;
    border: 1px solid rgba(200, 168, 130, 0.25);
    border-radius: 4px;
    padding: 8px 12px;
    font-family: var(--font-serif);
    font-size: 0.8125rem;
    color: var(--color-ink-light);
    outline: none;
    transition: border-color 0.2s;
  }

  .feedback-input:focus {
    border-color: rgba(200, 168, 130, 0.5);
  }

  .feedback-hint {
    display: flex;
    gap: 12px;
    margin-top: 8px;
  }

  .hint-btn {
    font-family: var(--font-serif);
    font-size: 0.7rem;
    color: var(--color-ink-faint);
    background: none;
    border: none;
    cursor: pointer;
    letter-spacing: 0.08em;
    transition: color 0.2s;
  }

  .hint-btn:hover {
    color: var(--color-ink-light);
  }
</style>
