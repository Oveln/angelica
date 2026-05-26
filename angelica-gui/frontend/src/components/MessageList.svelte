<script lang="ts">
  import type { Message } from '$lib/types';
  import MessageBubble from './MessageBubble.svelte';

  let { messages, selectedId }: { messages: Message[]; selectedId: string | null } = $props();
</script>

<div class="max-w-2xl mx-auto px-6 py-8">
  {#each messages as msg, i (msg.id)}
    {#if i > 0 && (messages[i - 1].role === 'assistant' || messages[i - 1].role === 'user') && (msg.role === 'assistant' || msg.role === 'user')}
      <div class="turn-sep">
        <div class="turn-sep-dot"></div>
      </div>
    {/if}
    <div class="msg-enter" style="margin-bottom: {msg.role === 'system' ? '2rem' : '0.75rem'}">
      <MessageBubble {msg} {selectedId} />
    </div>
  {/each}
</div>
