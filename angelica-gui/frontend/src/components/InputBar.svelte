<script lang="ts">
  let { text = $bindable(''), isLoading = false, onSend }: {
    text: string;
    isLoading: boolean;
    onSend: () => void;
  } = $props();

  function handleKeydown(e: KeyboardEvent) {
    // e.keyCode 229 = IME is processing this keystroke
    if (e.key === 'Enter' && !e.shiftKey && e.keyCode !== 229) {
      e.preventDefault();
      onSend();
    }
  }
</script>

<div class="px-6 pb-5 pt-3">
  <div class="max-w-2xl mx-auto">
    <textarea
      class="w-full bg-transparent text-[1rem] placeholder-opacity-30 resize-none focus:outline-none"
      style="color: var(--color-ink); border-bottom: 1px solid {text.trim() ? 'var(--color-amber-faint)' : 'var(--color-ink-dark)'}; padding: 0.5em 0; transition: border-color 0.3s;"
      rows="1"
      placeholder="说些什么..."
      bind:value={text}
      onkeydown={handleKeydown}
      disabled={isLoading}
    ></textarea>
    <div class="flex justify-end mt-2">
      <button
        class="text-[0.75rem] tracking-[0.1em] transition-colors duration-200"
        style="color: {isLoading || !text.trim() ? 'var(--color-ink-dark)' : 'var(--color-amber)'}; cursor: {isLoading || !text.trim() ? 'default' : 'pointer'};"
        onclick={onSend}
        disabled={isLoading || !text.trim()}
      >
        发送
      </button>
    </div>
  </div>
</div>
