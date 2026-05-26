<script lang="ts">
  import { onMount, tick } from 'svelte';
  import {
    onAppEvent,
    sendMessage,
    approvePending,
    approveAlways,
    rejectTool,
    type ApprovalPending as ApprovalPayload,
    type ToolCalling,
    type ToolResult,
    type FatigueUpdate,
    type InitEvent,
    type InitMessage,
  } from '$lib/api';
  import type { Message } from '$lib/types';
  import { genId } from '$lib/types';
  import MessageList from './components/MessageList.svelte';
  import InputBar from './components/InputBar.svelte';
  import StatusBar from './components/StatusBar.svelte';
  import ApprovalPanel from './components/ApprovalPanel.svelte';

  let messages = $state<Message[]>([]);
  let currentAssistant: Message | null = null;
  let inputText = $state('');
  let isLoading = $state(false);
  let fatigue = $state(0);
  let statusText = $state('');
  let approval = $state<ApprovalPayload | null>(null);
  let listEl: HTMLDivElement | undefined = $state();
  let scrollTick = $state(0);

  const unsubs: (() => void)[] = [];

  let initLoaded = $state(false);

  onMount(async () => {
    unsubs.push(await onAppEvent('init', (p: InitEvent) => {
      if (!initLoaded) {
        messages = convertHistoryToMessages(p.messages);
        initLoaded = true;
      }
    }));

    unsubs.push(await onAppEvent('text-delta', (p) => {
      ensureAssistant();
      if (currentAssistant) currentAssistant.content += p.delta;
      isLoading = true;
      scrollTick++;
    }));

    unsubs.push(await onAppEvent('thinking-delta', (p) => {
      ensureAssistant();
      if (currentAssistant) currentAssistant.thinking += p.delta;
      scrollTick++;
    }));

    unsubs.push(await onAppEvent('text-done', () => {
      if (currentAssistant) {
        currentAssistant.done = true;
        currentAssistant = null;
      }
      isLoading = false;
      scrollTick++;
    }));

    unsubs.push(await onAppEvent('turn-complete', () => {
      if (currentAssistant) {
        currentAssistant.done = true;
        currentAssistant = null;
      }
      isLoading = false;
      scrollTick++;
    }));

    unsubs.push(await onAppEvent('tool-calling', (p: ToolCalling) => {
      ensureAssistant();
      if (currentAssistant) {
        currentAssistant.toolCalls.push({
          callId: p.call_id,
          name: p.name,
          arguments: p.arguments,
          pending: true,
        });
        scrollTick++;
      }
    }));

    unsubs.push(await onAppEvent('tool-result', (p: ToolResult) => {
      if (currentAssistant) {
        const tc = currentAssistant.toolCalls.find((t) => t.callId === p.call_id);
        if (tc) {
          tc.result = p.result;
          tc.diffPreview = p.diff_preview;
          tc.pending = false;
          scrollTick++;
        }
      }
    }));

    unsubs.push(await onAppEvent('approval-pending', (p: ApprovalPayload) => {
      approval = p;
    }));

    unsubs.push(await onAppEvent('tool-rejected', () => {
      approval = null;
    }));

    unsubs.push(await onAppEvent('error', (p) => {
      messages.push({
        id: genId(),
        role: 'system',
        content: p.message,
        thinking: '',
        toolCalls: [],
        timestamp: Date.now(),
        done: true,
      });
      isLoading = false;
      scrollTick++;
    }));

    unsubs.push(await onAppEvent('fatigue-update', (p: FatigueUpdate) => {
      fatigue = p.fatigue;
      statusText = p.desc;
    }));

    unsubs.push(await onAppEvent('falling-asleep', () => {
      statusText = '正在入睡...';
    }));

    unsubs.push(await onAppEvent('sleeping', () => {
      statusText = '沉睡中';
    }));

    unsubs.push(await onAppEvent('waking-up', (p) => {
      statusText = '苏醒';
      messages.push({
        id: genId(),
        role: 'system',
        content: p.dream,
        thinking: '',
        toolCalls: [],
        timestamp: Date.now(),
        done: true,
      });
      scrollTick++;
    }));
  });

  function ensureAssistant() {
    if (!currentAssistant) {
      const id = genId();
      messages.push({
        id,
        role: 'assistant',
        content: '',
        thinking: '',
        toolCalls: [],
        timestamp: Date.now(),
        done: false,
      });
      currentAssistant = messages[messages.length - 1];
    }
  }

  function convertHistoryToMessages(entries: InitMessage[]): Message[] {
    const results: Message[] = [];
    for (const entry of entries) {
      if (entry.role === 'system') continue;
      if (entry.role === 'user') {
        results.push({
          id: genId(),
          role: 'user',
          content: entry.content ?? '',
          thinking: '',
          toolCalls: [],
          timestamp: Date.now(),
          done: true,
        });
      } else if (entry.role === 'assistant') {
        const hasToolCalls = entry.tool_calls && entry.tool_calls.length > 0;
        results.push({
          id: genId(),
          role: 'assistant',
          content: entry.content ?? '',
          thinking: entry.reasoning_content ?? '',
          toolCalls: hasToolCalls
            ? entry.tool_calls!.map((tc) => ({
                callId: tc.id,
                name: tc.function.name,
                arguments: tc.function.arguments,
                pending: true,
              }))
            : [],
          timestamp: Date.now(),
          done: true,
        });
      } else if (entry.role === 'tool') {
        const tcId = entry.tool_call_id;
        if (tcId) {
          for (let i = results.length - 1; i >= 0; i--) {
            const tc = results[i].toolCalls.find((t) => t.callId === tcId);
            if (tc) {
              tc.result = entry.content ?? '';
              tc.pending = false;
              break;
            }
          }
        }
      }
    }
    return results;
  }

  async function handleSend() {
    const text = inputText.trim();
    if (!text || isLoading) return;

    messages.push({
      id: genId(),
      role: 'user',
      content: text,
      thinking: '',
      toolCalls: [],
      timestamp: Date.now(),
      done: true,
    });

    inputText = '';
    isLoading = true;
    scrollTick++;

    await sendMessage(text);
    await scrollToBottom();
  }

  async function handleApprove() {
    if (!approval) return;
    await approvePending();
    approval = null;
  }

  async function handleApproveAlways() {
    if (!approval) return;
    await approveAlways(approval.tool_name, approval.tool_target ?? '', true);
    approval = null;
  }

  async function handleReject(feedback?: string) {
    await rejectTool(feedback);
    approval = null;
  }

  $effect(() => {
    scrollTick;
    if (listEl) scheduleScroll();
  });

  let scrollPending = false;

  function scheduleScroll() {
    if (scrollPending) return;
    scrollPending = true;
    requestAnimationFrame(() => {
      scrollPending = false;
      if (listEl) {
        listEl.scrollTop = listEl.scrollHeight;
      }
    });
  }

  async function scrollToBottom() {
    await tick();
    if (listEl) listEl.scrollTop = listEl.scrollHeight;
  }
</script>

<div class="flex flex-col h-screen app-bg">
  <header class="px-6 pt-6 pb-3">
    <div class="max-w-2xl mx-auto">
      <div class="flex items-center justify-between">
        <div class="flex items-baseline gap-3">
          <h1 class="text-[0.95rem] font-normal tracking-[0.2em]" style="color: var(--color-amber);">祈芷</h1>
          <span class="text-[0.75rem] tracking-[0.1em]" style="color: var(--color-ink-dark);">angelica</span>
        </div>
        <StatusBar {fatigue} {statusText} {isLoading} />
      </div>
      <div class="header-line mt-4"></div>
    </div>
  </header>

  <div class="flex-1 overflow-y-auto" bind:this={listEl}>
    <MessageList {messages} selectedId={null} />
  </div>

  {#if approval}
    <ApprovalPanel
      {approval}
      onApprove={handleApprove}
      onApproveAlways={handleApproveAlways}
      onReject={handleReject}
    />
  {/if}

  <InputBar bind:text={inputText} {isLoading} onSend={handleSend} />
</div>
