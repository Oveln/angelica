<script lang="ts">
  import { BUILTIN_COMMANDS, type SlashCommand } from '$lib/commands.svelte';

  let {
    filter = '',
    onSelect,
    onClose,
  }: {
    filter: string;
    onSelect: (cmd: SlashCommand) => void;
    onClose: () => void;
  } = $props();

  let selectedIndex = $state(0);

  let stripped = $derived(filter.startsWith('/') ? filter.slice(1) : filter);
  let matched = $derived.by(() => {
    const results: SlashCommand[] = [];
    for (const cmd of BUILTIN_COMMANDS) {
      if (cmd.name.startsWith(stripped) || cmd.aliases.some(a => a.startsWith(stripped))) {
        results.push(cmd);
      }
    }
    return results;
  });

  $effect(() => {
    matched;
    if (selectedIndex >= matched.length) {
      selectedIndex = 0;
    }
  });

  function handleKeydown(e: KeyboardEvent) {
    if (e.target instanceof HTMLInputElement || e.target instanceof HTMLTextAreaElement) {
      if (
        e.key === 'Escape' ||
        e.key === 'Enter' ||
        e.key === 'Tab' ||
        e.key === 'ArrowUp' ||
        e.key === 'ArrowDown'
      ) {
        // intercept navigation keys even when textarea is focused
      } else {
        return;
      }
    }

    if (matched.length === 0) {
      if (e.key === 'Escape') {
        e.preventDefault();
        onClose();
      }
      return;
    }

    switch (e.key) {
      case 'ArrowUp':
        e.preventDefault();
        if (selectedIndex > 0) selectedIndex--;
        break;
      case 'ArrowDown':
        e.preventDefault();
        if (selectedIndex < matched.length - 1) selectedIndex++;
        break;
      case 'Tab':
        e.preventDefault();
        if (matched[selectedIndex]) {
          onSelect(matched[selectedIndex]);
        }
        break;
      case 'Enter':
        e.preventDefault();
        if (matched[selectedIndex]) {
          onSelect(matched[selectedIndex]);
        }
        break;
      case 'Escape':
        e.preventDefault();
        onClose();
        break;
    }
  }
</script>

<svelte:window onkeydown={handleKeydown} />

{#if matched.length > 0}
  <div class="slash-menu">
    {#each matched as cmd, i (cmd.name)}
      <button
        class="slash-item {i === selectedIndex ? 'selected' : ''}"
        onclick={() => onSelect(cmd)}
        onmouseenter={() => selectedIndex = i}
      >
        <span class="slash-name">
          /{cmd.name}
          {#if cmd.aliases.length > 0}
            <span class="slash-aliases">({cmd.aliases.join(', ')})</span>
          {/if}
        </span>
        <span class="slash-desc">{cmd.description}</span>
      </button>
    {/each}
  </div>
{/if}

<style>
  .slash-menu {
    position: absolute;
    bottom: 100%;
    left: 0;
    right: 0;
    max-height: 280px;
    overflow-y: auto;
    background: #0c0c0c;
    border: 1px solid #1e1e1e;
    border-radius: 8px;
    box-shadow: 0 -8px 32px rgba(0,0,0,0.5);
    z-index: 50;
    padding: 4px;
  }

  .slash-item {
    display: flex;
    align-items: center;
    gap: 12px;
    width: 100%;
    padding: 8px 12px;
    background: transparent;
    border: none;
    border-radius: 4px;
    cursor: pointer;
    text-align: left;
    color: var(--color-ink-faint);
    transition: background 0.15s;
  }

  .slash-item:hover,
  .slash-item.selected {
    background: rgba(200, 168, 130, 0.1);
  }

  .slash-item.selected .slash-name {
    color: var(--color-amber);
  }

  .slash-name {
    font-family: var(--font-mono);
    font-size: 0.8rem;
    min-width: 180px;
    color: var(--color-ink-light);
  }

  .slash-aliases {
    color: var(--color-ink-dark);
    font-size: 0.7rem;
    margin-left: 4px;
  }

  .slash-desc {
    font-size: 0.75rem;
    color: var(--color-ink-dark);
  }

  .slash-item.selected .slash-desc {
    color: var(--color-ink-light);
  }
</style>
