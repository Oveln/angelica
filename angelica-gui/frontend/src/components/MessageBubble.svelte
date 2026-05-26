<script lang="ts">
  import type { Message } from '$lib/types';
  import { renderMarkdown } from '$lib/markdown';

  let { msg, selectedId }: { msg: Message; selectedId: string | null } = $props();

  let rendered = $state('');
  let thinkingOpen = $state(true);

  $effect(() => {
    if (msg.done && msg.role === 'assistant') {
      rendered = renderMarkdown(msg.content);
    } else {
      rendered = '';
    }
  });
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
    {:else}
      <p class="text-[1rem] leading-relaxed whitespace-pre-wrap" style="color: {msg.role === 'user' ? '#e2dcd4' : 'var(--color-ink)'};">
        {msg.content}
      </p>
    {/if}

    {#if msg.role === 'assistant' && !msg.done}
      <span class="stream-cursor"></span>
    {/if}

    {#if msg.toolCalls.length > 0}
      <div class="mt-3 space-y-1">
        {#each msg.toolCalls as tc (tc.callId)}
          <div class="flex items-start gap-2 text-xs" style="font-family: var(--font-mono); color: var(--color-ink-faint);">
            <span style="color: var(--color-amber);">{tc.name}</span>
            {#if tc.pending}
              <span class="animate-pulse" style="color: var(--color-ink-dark);">···</span>
            {:else if tc.result}
              <details>
                <summary class="cursor-pointer hover:text-ink-light transition-colors">result</summary>
                <pre class="mt-1 max-h-40 overflow-auto whitespace-pre-wrap text-[0.7rem]" style="color: var(--color-ink-light);">{tc.result}</pre>
              </details>
            {/if}
          </div>
        {/each}
      </div>
    {/if}
  </div>
{/if}
