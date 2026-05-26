<script lang="ts">
  import type { ChatMessage } from '$lib/types';
  import { renderMarkdown } from '$lib/markdown';

  let { msg, thinkingVisible = true }: { msg: ChatMessage; thinkingVisible: boolean } = $props();

  let thinkingOpen = $state(thinkingVisible);

  let rendered = $derived(
    msg.role === 'assistant' && msg.content ? renderMarkdown(msg.content) : ''
  );
</script>

{#if msg.role === 'system'}
  <p class="text-center text-sm italic tracking-wide" style="color: var(--color-amber-muted);">
    {msg.content}
  </p>
{:else}
  <div class="text-center">
    <span class="text-[0.7rem] tracking-[0.15em]" style="color: var(--color-ink-faint);">
      {msg.role === 'user' ? '你' : '祈芷'}
    </span>
  </div>

  <div class="mt-3 max-w-2xl mx-auto">
    {#if msg.thinking}
      <div class="mb-3">
        <button
          class="thinking-toggle"
          onclick={() => thinkingOpen = !thinkingOpen}
        >
          <span class="arrow {thinkingOpen ? 'open' : ''}">▸</span>
          思考
        </button>
        {#if thinkingOpen}
          <pre class="thinking-area mt-1.5 whitespace-pre-wrap max-h-64 overflow-y-auto">{msg.thinking}</pre>
        {/if}
      </div>
    {/if}

    {#if msg.role === 'assistant' && rendered}
      <div class="prose-ink max-w-none">
        {@html rendered}
      </div>
    {:else if msg.content}
      <p class="text-[1rem] leading-relaxed whitespace-pre-wrap" style="color: {msg.role === 'user' ? '#e2dcd4' : 'var(--color-ink)'};">
        {msg.content}
      </p>
    {/if}
  </div>
{/if}
