<script lang="ts">
  let { fatigue = 0, statusText = '', isLoading = false }: {
    fatigue: number;
    statusText: string;
    isLoading: boolean;
  } = $props();

  let fatiguePercent = $derived(Math.round(fatigue * 100));
</script>

<div class="flex items-center gap-3 text-[0.75rem] tracking-[0.08em]" style="color: var(--color-ink-faint);">
  {#if isLoading}
    <span style="color: var(--color-ink-faint);">
      ·
    </span>
  {/if}

  {#if statusText}
    <span>{statusText}</span>
  {/if}

  {#if fatigue > 0}
    <div class="flex items-center gap-1.5">
      <span>{fatiguePercent}%</span>
    </div>
  {/if}
</div>
