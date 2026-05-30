<script lang="ts">
  import type { ChatMessage } from '$lib/types';
  import { renderMarkdown } from '$lib/markdown';

  let {
    msg,
    thinkingVisible = true,
  }: {
    msg: ChatMessage;
    thinkingVisible: boolean;
  } = $props();

  let thinkingOpen = $state(thinkingVisible);

  // Debounced markdown rendering — avoid calling marked.parse() on every
  // streaming delta which can cause jank on long responses.
  let rendered = $state('');
  let renderTimer: ReturnType<typeof setTimeout> | undefined;

  $effect(() => {
    const content = msg.content;
    if (!content) { rendered = ''; return; }
    clearTimeout(renderTimer);
    renderTimer = setTimeout(() => { rendered = renderMarkdown(content); }, 150);
    // Immediate render for short or initial content to avoid perceived lag
    if (content.length < 200 || !rendered) {
      rendered = renderMarkdown(content);
    }
  });
</script>

<div class="streaming-bubble">
  <div class="text-center">
    <span class="text-[0.7rem] tracking-[0.15em]" style="color: var(--color-ink-faint);">祈芷</span>
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

    {#if rendered}
      <div class="prose-ink max-w-none">
        {@html rendered}
      </div>
    {/if}

    <span class="stream-cursor"></span>
  </div>
</div>

<style>
  .streaming-bubble {
    animation: fade-in-simple 0.3s ease-out;
  }
</style>
