<script lang="ts">
  import type { Message, ChatMessage } from '$lib/types';
  import MessageBubble from './MessageBubble.svelte';
  import ToolCard from './ToolCard.svelte';
  import StreamingBubble from './StreamingBubble.svelte';

  let {
    messages,
    thinkingVisible,
    thinkingBuffer = '',
    textBuffer = '',
    isStreaming = false,
  }: {
    messages: Message[];
    thinkingVisible: boolean;
    thinkingBuffer?: string;
    textBuffer?: string;
    isStreaming?: boolean;
  } = $props();

  let showStreaming = $derived(isStreaming && (thinkingBuffer || textBuffer));

  let streamingMsg = $derived<ChatMessage>({
    type: 'chat',
    id: '__streaming__',
    role: 'assistant',
    content: textBuffer,
    thinking: thinkingBuffer,
    timestamp: Date.now(),
  });
</script>

<div class="max-w-2xl mx-auto px-6 py-8">
  {#each messages as msg (msg.id)}
    {#if msg.type === 'chat'}
      {#if msg.role !== 'system'}
        <div class="turn-sep">
          <div class="turn-sep-dot"></div>
        </div>
      {/if}
      <div class="msg-enter" style="margin-bottom: {msg.role === 'system' ? '2rem' : '0.75rem'}">
        <MessageBubble msg={msg} {thinkingVisible} />
      </div>
    {:else}
      <div class="tool-enter" style="margin: 0.5rem 0;">
        <ToolCard tool={msg} />
      </div>
    {/if}
  {/each}

  {#if showStreaming}
    <div class="turn-sep">
      <div class="turn-sep-dot"></div>
    </div>
    <div style="margin-bottom: 0.75rem;">
      <StreamingBubble msg={streamingMsg} {thinkingVisible} />
    </div>
  {/if}
</div>

<style>
  .turn-sep {
    display: flex;
    justify-content: center;
    padding: 12px 0 8px;
  }

  .turn-sep-dot {
    width: 3px;
    height: 3px;
    border-radius: 50%;
    background: var(--color-ink-dark);
  }

  .msg-enter {
    animation: fade-in 0.3s ease-out;
  }

  .tool-enter {
    animation: fade-in 0.2s ease-out;
  }

  @keyframes fade-in {
    from { opacity: 0; }
    to { opacity: 1; }
  }
</style>
