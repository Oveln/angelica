<script lang="ts">
  let {
    text = $bindable(''),
    disabled = false,
    onSend,
    onKeydown,
    onInputChange,
  }: {
    text: string;
    disabled: boolean;
    onSend: () => void;
    onKeydown: (e: KeyboardEvent) => void;
    onInputChange: () => void;
  } = $props();

  let textareaEl: HTMLTextAreaElement | undefined = $state();

  function handleInput() {
    if (!textareaEl) return;
    textareaEl.style.height = 'auto';
    textareaEl.style.height = Math.min(textareaEl.scrollHeight, 200) + 'px';
    onInputChange();
  }
</script>

<div class="input-bar">
  <div style="max-width: 640px; margin: 0 auto;">
    <textarea
      bind:this={textareaEl}
      class="input-field"
      rows="1"
      placeholder={disabled ? '' : '说些什么...  / 查看命令'}
      bind:value={text}
      onkeydown={onKeydown}
      oninput={handleInput}
      {disabled}
    ></textarea>
  </div>
  <div class="input-footer">
    <span class="input-hint">Shift+Enter 换行</span>
    <button
      class="send-btn"
      class:disabled={disabled || !text.trim()}
      onclick={onSend}
      disabled={disabled || !text.trim()}
    >
      发送
    </button>
  </div>
</div>

<style>
  .input-bar {
    padding: 8px 24px 16px;
  }

  .input-field {
    width: 100%;
    background: transparent;
    border: none;
    border-bottom: 1px solid var(--color-ink-dark);
    padding: 8px 0;
    font-family: var(--font-serif);
    font-size: 1rem;
    color: var(--color-ink);
    resize: none;
    outline: none;
    transition: border-color 0.3s, opacity 0.3s;
    line-height: 1.6;
  }

  .input-field:focus {
    border-bottom-color: var(--color-amber-faint);
  }

  .input-field:disabled {
    opacity: 0.3;
  }

  .input-field::placeholder {
    color: var(--color-ink-dark);
  }

  .input-footer {
    max-width: 640px;
    margin: 0 auto;
    display: flex;
    justify-content: space-between;
    align-items: center;
    margin-top: 8px;
  }

  .input-hint {
    font-size: 0.6rem;
    color: var(--color-ink-dark);
    letter-spacing: 0.05em;
  }

  .send-btn {
    font-family: var(--font-serif);
    font-size: 0.75rem;
    letter-spacing: 0.1em;
    color: var(--color-amber);
    background: none;
    border: none;
    cursor: pointer;
    transition: opacity 0.2s;
    padding: 4px 8px;
  }

  .send-btn:hover:not(.disabled) {
    opacity: 0.8;
  }

  .send-btn.disabled {
    color: var(--color-ink-dark);
    cursor: default;
  }
</style>
