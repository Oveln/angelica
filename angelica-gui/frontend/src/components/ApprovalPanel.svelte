<script lang="ts">
  import type { ApprovalPending as ApprovalPayload } from '$lib/api';

  let {
    approval,
    onApprove,
    onApproveAlways,
    onReject,
  }: {
    approval: ApprovalPayload;
    onApprove: () => void;
    onApproveAlways: () => void;
    onReject: (feedback?: string) => void;
  } = $props();

  let rejectFeedback = $state('');
  let showRejectInput = $state(false);
</script>

<div class="px-6 py-3 msg-enter" style="border-top: 1px solid var(--color-amber-faint);">
  <div class="max-w-2xl mx-auto">
    <p class="text-[0.8125rem] tracking-wide" style="color: var(--color-amber);">
      审批 · {approval.tool_name}
      {#if approval.tool_target}
        <span style="color: var(--color-ink-faint);">→ {approval.tool_target}</span>
      {/if}
    </p>
    <pre class="mt-1.5 max-h-32 overflow-auto whitespace-pre-wrap text-[0.75rem]" style="color: var(--color-ink-faint); font-family: var(--font-mono);">{approval.preview}</pre>

    <div class="flex items-center gap-4 mt-3">
      <button
        class="text-[0.75rem] tracking-[0.08em] transition-colors duration-200 hover:opacity-80"
        style="color: var(--color-amber);"
        onclick={onApprove}
      >
        允许
      </button>
      <button
        class="text-[0.75rem] tracking-[0.08em] transition-colors duration-200 hover:opacity-80"
        style="color: var(--color-ink-light);"
        onclick={onApproveAlways}
      >
        始终允许
      </button>
      <button
        class="text-[0.75rem] tracking-[0.08em] transition-colors duration-200 hover:opacity-80"
        style="color: var(--color-ink-faint);"
        onclick={() => showRejectInput = !showRejectInput}
      >
        拒绝
      </button>
    </div>

    {#if showRejectInput}
      <div class="flex items-center gap-3 mt-2.5">
        <input
          type="text"
          class="flex-1 bg-transparent text-[0.8125rem] focus:outline-none"
          style="color: var(--color-ink-light); border-bottom: 1px solid var(--color-ink-dark); padding: 0.3em 0;"
          placeholder="拒绝原因（可选）"
          bind:value={rejectFeedback}
        />
        <button
          class="text-[0.75rem] tracking-[0.08em] hover:opacity-80"
          style="color: var(--color-ink-faint);"
          onclick={() => onReject(rejectFeedback || undefined)}
        >
          确认
        </button>
      </div>
    {/if}
  </div>
</div>
