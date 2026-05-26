<script lang="ts">
  import type { SessionUsage } from '$lib/api';

  let {
    sessions,
    onClose,
  }: {
    sessions: SessionUsage[];
    onClose: () => void;
  } = $props();

  function handleKeydown(e: KeyboardEvent) {
    if (e.target instanceof HTMLInputElement || e.target instanceof HTMLTextAreaElement) return;
    if (e.key === 'Escape') {
      e.preventDefault();
      onClose();
    }
  }

  function formatTokens(n: number): string {
    if (n >= 1_000_000) return (n / 1_000_000).toFixed(1) + 'M';
    return (n / 1000).toFixed(1) + 'k';
  }

  function formatTime(t: string): string {
    return t.slice(0, 16).replace('T', ' ');
  }

  let maxTotal = $derived(
    sessions.reduce((max, s) => Math.max(max, s.total_tokens), 0) || 1
  );
</script>

<svelte:window onkeydown={handleKeydown} />

<div class="overlay" onclick={onClose} role="presentation">
  <!-- svelte-ignore a11y_click_events_have_key_events -->
  <div class="modal" onclick={(e) => e.stopPropagation()} role="dialog" tabindex="-1">
    <div class="modal-header">
      <h2 class="modal-title">Token 用量统计</h2>
      <button class="close-btn" onclick={onClose}>&times;</button>
    </div>

    {#if sessions.length === 0}
      <div class="empty">
        <p>尚无数据。开始对话后即可查看用量。</p>
      </div>
    {:else}
      <div class="stats-summary">
        <div class="stat-card">
          <span class="stat-value">{sessions.length}</span>
          <span class="stat-label">会话</span>
        </div>
        <div class="stat-card">
          <span class="stat-value">{formatTokens(sessions.reduce((a, s) => a + s.total_tokens, 0))}</span>
          <span class="stat-label">总计</span>
        </div>
        <div class="stat-card">
          <span class="stat-value">{formatTokens(sessions.reduce((a, s) => a + s.completion_tokens, 0))}</span>
          <span class="stat-label">输出</span>
        </div>
      </div>

      <div class="sparklines">
        <div class="sparkline-row">
          <span class="spark-label">Total</span>
          <div class="spark-bar-container">
            {#each sessions.slice(-30) as s}
              <div
                class="spark-bar amber"
                style="height: {Math.max(2, (s.total_tokens / maxTotal) * 100)}%"
                title="{formatTokens(s.total_tokens)}"
              ></div>
            {/each}
          </div>
        </div>
      </div>

      <div class="table-wrapper">
        <table class="usage-table">
          <thead>
            <tr>
              <th>#</th>
              <th>类型</th>
              <th class="right">输入</th>
              <th class="right">输出</th>
              <th class="right">总计</th>
              <th class="right">轮次</th>
              <th class="right">缓存</th>
              <th>时间</th>
            </tr>
          </thead>
          <tbody>
            {#each sessions.toSorted((a, b) => b.start_time.localeCompare(a.start_time)) as s, i (s.start_time)}
              <tr>
                <td>{i + 1}</td>
                <td>
                  <span class="scope-badge {s.scope}">
                    {s.scope === 'awake' ? '● 清醒' : '○ 沉睡'}
                  </span>
                </td>
                <td class="right mono">{formatTokens(s.prompt_tokens)}</td>
                <td class="right mono">{formatTokens(s.completion_tokens)}</td>
                <td class="right mono">{formatTokens(s.total_tokens)}</td>
                <td class="right mono">{s.iterations}</td>
                <td class="right mono">
                  {#if s.cache_hit_tokens + s.cache_miss_tokens > 0}
                    {Math.round((s.cache_hit_tokens / (s.cache_hit_tokens + s.cache_miss_tokens)) * 100)}%
                  {:else}
                    --
                  {/if}
                </td>
                <td class="time">{formatTime(s.start_time)}</td>
              </tr>
            {/each}
          </tbody>
        </table>
      </div>

      <div class="modal-footer">
        <span class="footer-hint">Esc 关闭</span>
      </div>
    {/if}
  </div>
</div>

<style>
  .overlay {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.7);
    display: flex;
    align-items: center;
    justify-content: center;
    z-index: 100;
    animation: fade-in-simple 0.2s ease-out;
  }

  .modal {
    background: #0a0a0a;
    border: 1px solid #1e1e1e;
    border-radius: 12px;
    width: 90vw;
    max-width: 800px;
    max-height: 85vh;
    display: flex;
    flex-direction: column;
    overflow: hidden;
    box-shadow: 0 16px 64px rgba(0, 0, 0, 0.5);
  }

  .modal-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 16px 20px;
    border-bottom: 1px solid #1a1a1a;
  }

  .modal-title {
    font-size: 0.9rem;
    font-weight: 500;
    color: var(--color-amber);
    letter-spacing: 0.1em;
    margin: 0;
  }

  .close-btn {
    font-size: 1.2rem;
    color: var(--color-ink-dark);
    background: none;
    border: none;
    cursor: pointer;
    padding: 4px 8px;
    transition: color 0.2s;
  }

  .close-btn:hover {
    color: var(--color-ink-light);
  }

  .empty {
    padding: 48px 24px;
    text-align: center;
    color: var(--color-ink-faint);
    font-size: 0.875rem;
  }

  .stats-summary {
    display: flex;
    gap: 16px;
    padding: 16px 20px;
    border-bottom: 1px solid #141414;
  }

  .stat-card {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  .stat-value {
    font-family: var(--font-mono);
    font-size: 1rem;
    color: var(--color-ink-light);
    font-weight: 500;
  }

  .stat-label {
    font-size: 0.65rem;
    color: var(--color-ink-dark);
    letter-spacing: 0.1em;
    text-transform: uppercase;
  }

  .sparklines {
    padding: 12px 20px;
    border-bottom: 1px solid #141414;
  }

  .sparkline-row {
    display: flex;
    align-items: flex-end;
    gap: 8px;
  }

  .spark-label {
    font-size: 0.65rem;
    color: var(--color-ink-dark);
    min-width: 40px;
  }

  .spark-bar-container {
    flex: 1;
    display: flex;
    align-items: flex-end;
    gap: 2px;
    height: 32px;
  }

  .spark-bar {
    flex: 1;
    min-width: 3px;
    border-radius: 1px 1px 0 0;
    transition: height 0.3s;
  }

  .spark-bar.amber {
    background: rgba(200, 168, 130, 0.5);
  }

  .table-wrapper {
    flex: 1;
    overflow-y: auto;
    padding: 0 20px;
  }

  .usage-table {
    width: 100%;
    border-collapse: collapse;
    font-size: 0.75rem;
  }

  .usage-table th {
    font-weight: 500;
    color: var(--color-ink-dark);
    text-align: left;
    padding: 10px 6px;
    border-bottom: 1px solid #1a1a1a;
    letter-spacing: 0.05em;
    font-size: 0.65rem;
    text-transform: uppercase;
    position: sticky;
    top: 0;
    background: #0a0a0a;
  }

  .usage-table th.right,
  .usage-table td.right {
    text-align: right;
  }

  .usage-table td {
    padding: 8px 6px;
    color: var(--color-ink-faint);
    border-bottom: 1px solid #0f0f0f;
  }

  .usage-table tr:hover td {
    background: rgba(200, 168, 130, 0.03);
  }

  .mono {
    font-family: var(--font-mono);
    font-size: 0.7rem;
  }

  .time {
    font-family: var(--font-mono);
    font-size: 0.65rem;
    color: var(--color-ink-dark);
  }

  .scope-badge {
    font-size: 0.7rem;
    letter-spacing: 0.03em;
  }

  .scope-badge.awake {
    color: #5a9e6f;
  }

  .scope-badge.sleep {
    color: var(--color-ink-dark);
  }

  .modal-footer {
    padding: 10px 20px;
    border-top: 1px solid #1a1a1a;
  }

  .footer-hint {
    font-size: 0.65rem;
    color: var(--color-ink-dark);
    letter-spacing: 0.05em;
  }
</style>
