<script lang="ts">
  import type { UsageMetrics } from '$lib/api';

  let {
    fatigue = 0,
    fatigueDesc = '',
    turns = 0,
    toolCalls = 0,
    statusText = '',
    isStreaming = false,
    modelName = '',
    usage = null,
    thinkingVisible = true,
    messageCount = 0,
  }: {
    fatigue: number;
    fatigueDesc: string;
    turns: number;
    toolCalls: number;
    statusText: string;
    isStreaming: boolean;
    modelName: string;
    usage: UsageMetrics | null;
    thinkingVisible: boolean;
    messageCount: number;
  } = $props();

  let fatiguePercent = $derived(Math.round(fatigue * 100));
  let modeLabel = $derived(
    isStreaming ? '● 思考中' :
    statusText ? statusText :
    '○ 空闲'
  );
  let modeClass = $derived(
    isStreaming ? 'streaming' :
    statusText ? 'status' :
    'idle'
  );

  function formatTokens(n: number): string {
    if (n >= 1_000_000) return (n / 1_000_000).toFixed(1) + 'M';
    return (n / 1000).toFixed(1) + 'k';
  }
</script>

<div class="status-bar">
  <div class="status-left">
    <span class="brand">祈芷</span>
    <span class="sep">│</span>
    {#if modelName}
      <span class="model-name">{modelName}</span>
      <span class="sep">│</span>
    {/if}
    <span class="mode {modeClass}">{modeLabel}</span>
    <span class="sep">│</span>
    <span class="stat">{messageCount} msgs</span>
  </div>
  <div class="status-right">
    {#if usage}
      <span class="stat">{formatTokens(usage.total_tokens)}</span>
      <span class="sep">│</span>
    {/if}
    {#if fatigue > 0}
      <div class="fatigue-bar">
        {#each Array(20) as _, i}
          <div class="fatigue-seg {i < Math.round(fatigue * 20) ? 'filled' : ''}"></div>
        {/each}
      </div>
      <span class="fatigue-pct">{fatiguePercent}%</span>
      <span class="sep">│</span>
    {/if}
    <span class="stat">think:{thinkingVisible ? 'on' : 'off'}</span>
  </div>
</div>

<style>
  .status-bar {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 0 24px;
    height: 24px;
    font-size: 0.7rem;
    letter-spacing: 0.06em;
    background: rgba(255, 255, 255, 0.02);
    border-top: 1px solid #0e0e0e;
    color: var(--color-ink-dark);
  }

  .status-left,
  .status-right {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .brand {
    color: var(--color-amber);
    font-weight: 500;
    letter-spacing: 0.15em;
  }

  .sep {
    color: #1a1a1a;
  }

  .model-name {
    font-family: var(--font-mono);
    font-size: 0.65rem;
    color: var(--color-ink-faint);
  }

  .mode {
    font-size: 0.65rem;
  }

  .mode.streaming {
    color: #5a9e6f;
  }

  .mode.status {
    color: var(--color-amber-muted);
  }

  .mode.idle {
    color: var(--color-ink-dark);
  }

  .stat {
    font-family: var(--font-mono);
    font-size: 0.6rem;
  }

  .fatigue-bar {
    display: flex;
    gap: 1px;
    align-items: center;
    height: 6px;
  }

  .fatigue-seg {
    width: 3px;
    height: 100%;
    background: #1a1a1a;
    border-radius: 1px;
    transition: background 0.3s;
  }

  .fatigue-seg.filled {
    background: var(--color-amber-muted);
  }

  .fatigue-pct {
    font-family: var(--font-mono);
    font-size: 0.6rem;
    color: var(--color-ink-faint);
    min-width: 28px;
  }
</style>
