<script lang="ts">
  import type { ToolMessage } from '$lib/types';
  import { renderDiff, isDiffContent } from '$lib/diff';

  let { tool }: { tool: ToolMessage } = $props();
  let open = $state(false);

  let hasDiff = $derived(!!tool.diffPreview && isDiffContent(tool.diffPreview));
  let diffHtml = $derived(hasDiff ? renderDiff(tool.diffPreview!) : '');
</script>

<div class="tool-card">
  <button class="tool-header" onclick={() => open = !open}>
    <span class="tool-arrow {open ? 'open' : ''}">▸</span>
    <span class="tool-name">{tool.name}</span>
    <span class="tool-args">{tool.display}</span>
    {#if tool.pending}
      <span class="tool-pending">···</span>
    {:else if !open}
      <span class="tool-done">✓</span>
    {/if}
  </button>
  {#if open && tool.result}
    <pre class="tool-result">{tool.result}</pre>
  {/if}
  {#if open && hasDiff}
    <pre class="tool-diff">{@html diffHtml}</pre>
  {/if}
</div>

<style>
  .tool-card {
    padding: 2px 0;
  }

  .tool-header {
    display: flex;
    align-items: center;
    gap: 6px;
    width: 100%;
    background: none;
    border: none;
    cursor: pointer;
    padding: 3px 0;
    font-family: var(--font-mono);
    font-size: 0.75rem;
    color: var(--color-ink-faint);
    transition: color 0.15s;
  }

  .tool-header:hover {
    color: var(--color-ink-light);
  }

  .tool-arrow {
    font-size: 0.6rem;
    color: var(--color-ink-dark);
    transition: transform 0.15s;
  }

  .tool-arrow.open {
    transform: rotate(90deg);
  }

  .tool-name {
    color: var(--color-amber);
  }

  .tool-args {
    color: var(--color-ink-dark);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .tool-pending {
    color: var(--color-amber-muted);
    animation: pulse 1.5s ease-in-out infinite;
  }

  @keyframes pulse {
    0%, 100% { opacity: 0.3; }
    50% { opacity: 1; }
  }

  .tool-done {
    color: #5a9e6f;
    font-size: 0.65rem;
  }

  .tool-result, .tool-diff {
    margin: 4px 0 0 14px;
    padding: 6px 8px;
    max-height: 160px;
    overflow: auto;
    white-space: pre-wrap;
    font-size: 0.7rem;
    color: var(--color-ink-light);
    background: rgba(255, 255, 255, 0.02);
    border-radius: 4px;
  }

  .tool-diff :global(.diff-add) {
    color: #6ec87a;
  }

  .tool-diff :global(.diff-del) {
    color: #e06070;
  }

  .tool-diff :global(.diff-hunk) {
    color: #6cb4d9;
  }

  .tool-diff :global(.diff-header) {
    color: var(--color-ink-dark);
  }

  .tool-diff :global(.diff-ctx) {
    color: var(--color-ink-faint);
  }
</style>
