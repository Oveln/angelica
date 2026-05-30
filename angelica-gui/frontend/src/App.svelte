<script lang="ts">
  import { onMount } from 'svelte';
  import {
    sendMessage,
    approvePending,
    approveAlways,
    rejectTool,
    requestUsageStats,
  } from '$lib/api';
  import { executeSlashCommand } from '$lib/commands.svelte';
  import { getStore } from '$lib/store.svelte';
  import MessageList from './components/MessageList.svelte';
  import InputBar from './components/InputBar.svelte';
  import StatusBar from './components/StatusBar.svelte';
  import ApprovalPanel from './components/ApprovalPanel.svelte';
  import SlashMenu from './components/SlashMenu.svelte';
  import UsageStats from './components/UsageStats.svelte';
  import SettingsPanel from './components/SettingsPanel.svelte';
  import { showSettingsPanel, closeSettingsPanel } from '$lib/commands.svelte';

  const settings = showSettingsPanel();

  const s = getStore();

  let inputText = $state('');
  let showSlashMenu = $state(false);
  let listEl: HTMLDivElement | undefined = $state();
  let autoScroll = $state(true);
  let scrollPending = false;

  onMount(() => {
    s.init();
    return () => s.destroy();
  });

  async function handleSend() {
    const text = inputText.trim();
    if (!text || s.inputDisabled) return;

    if (text.startsWith('/')) {
      executeSlashCommand(text.slice(1));
      inputText = '';
      showSlashMenu = false;
      return;
    }

    s.addUserMessage(text);
    inputText = '';
    showSlashMenu = false;
    scrollToBottom();
    await sendMessage(text);
  }

  async function handleApprove() {
    s.clearApproval();
    try { await approvePending(); } catch (e) { console.error('approve failed:', e); }
  }

  async function handleApproveAlwaysSession() {
    const { tool_name, tool_target } = s.approval!;
    s.clearApproval();
    try { await approveAlways(tool_name, tool_target ?? '*', false); } catch (e) { console.error('approve_always failed:', e); }
  }

  async function handleApproveAlwaysPersist() {
    const { tool_name, tool_target } = s.approval!;
    s.clearApproval();
    try { await approveAlways(tool_name, tool_target ?? '*', true); } catch (e) { console.error('approve_always failed:', e); }
  }

  async function handleReject(feedback?: string) {
    s.clearApproval();
    try { await rejectTool(feedback); } catch (e) { console.error('reject failed:', e); }
  }

  function handleSlashSelect(cmd: { name: string }) {
    inputText = `/${cmd.name} `;
    showSlashMenu = false;
  }

  function handleSlashClose() {
    showSlashMenu = false;
    inputText = '';
  }

  function handleInputKeydown(e: KeyboardEvent) {
    if (showSlashMenu) return;
    if (s.approval) return;

    if (e.key === 'Enter' && !e.shiftKey && e.keyCode !== 229) {
      e.preventDefault();
      handleSend();
      return;
    }

    if (e.key === '/' && inputText === '') {
      showSlashMenu = true;
    }
  }

  function handleInputChange() {
    if (inputText.startsWith('/') && inputText.length <= 20) {
      showSlashMenu = true;
    } else if (!inputText.startsWith('/')) {
      showSlashMenu = false;
    }
  }

  function handleListScroll() {
    if (!listEl) return;
    const threshold = 150;
    const distanceFromBottom = listEl.scrollHeight - listEl.scrollTop - listEl.clientHeight;
    autoScroll = distanceFromBottom <= threshold;
  }

  $effect(() => {
    s.messages;
    s.textBuffer;
    s.thinkingBuffer;
    if (autoScroll) scrollToBottom();
  });

  function scrollToBottom() {
    if (!listEl) return;
    if (scrollPending) return;
    scrollPending = true;
    requestAnimationFrame(() => {
      if (listEl) listEl.scrollTop = listEl.scrollHeight;
      scrollPending = false;
    });
  }
</script>

<div class="flex flex-col h-screen app-bg">
  {#if s.connectionLost}
    <div class="connection-lost-banner">
      Connection to agent lost. Please restart the application.
    </div>
  {/if}
  <header class="px-6 pt-6 pb-3">
    <div class="max-w-2xl mx-auto">
      <div class="flex items-center justify-between">
        <div class="flex items-baseline gap-3">
          <h1 class="text-[0.95rem] font-normal tracking-[0.2em]" style="color: var(--color-amber);">祈芷</h1>
          <span class="text-[0.75rem] tracking-[0.1em]" style="color: var(--color-ink-dark);">angelica</span>
        </div>
        <button
          class="text-[0.7rem] tracking-[0.08em] transition-colors duration-200 hover:opacity-80"
          style="color: var(--color-ink-dark); background: none; border: none; cursor: pointer;"
          onclick={requestUsageStats}
          title="Token 用量统计"
        >
          stats
        </button>
      </div>
      <div class="header-line mt-4"></div>
    </div>
  </header>

  <div class="flex-1 overflow-y-auto" bind:this={listEl} onscroll={handleListScroll}>
    <MessageList
      messages={s.messages}
      thinkingVisible={s.thinkingVisible}
      thinkingBuffer={s.thinkingBuffer}
      textBuffer={s.textBuffer}
      isStreaming={s.isStreaming}
    />
  </div>

  {#if !autoScroll}
    <button
      class="scroll-to-bottom-btn"
      onclick={() => { autoScroll = true; scrollToBottom(); }}
      title="Scroll to bottom"
    >
      ↓
    </button>
  {/if}

  {#if s.approval}
    <ApprovalPanel
      approval={s.approval}
      onApprove={handleApprove}
      onApproveAlwaysSession={handleApproveAlwaysSession}
      onApproveAlwaysPersist={handleApproveAlwaysPersist}
      onReject={handleReject}
    />
  {/if}

  <div class="input-area">
    {#if showSlashMenu}
      <div class="slash-wrapper">
        <SlashMenu
          filter={inputText}
          onSelect={handleSlashSelect}
          onClose={handleSlashClose}
        />
      </div>
    {/if}
    <InputBar
      bind:text={inputText}
      disabled={s.inputDisabled}
      isStreaming={s.isStreaming}
      approvalPending={s.approval !== null}
      onSend={handleSend}
      onKeydown={handleInputKeydown}
      onInputChange={handleInputChange}
    />
  </div>

  <StatusBar
    fatigue={s.fatigue}
    fatigueDesc={s.fatigueDesc}
    turns={s.fatigueTurns}
    toolCalls={s.fatigueToolCalls}
    statusText={s.statusText}
    isStreaming={s.isStreaming}
    modelName={s.modelName}
    usage={s.usage}
    thinkingVisible={s.thinkingVisible}
    messageCount={s.messages.length}
  />

  {#if s.showUsageStats}
    <UsageStats
      sessions={s.usageSessions}
      onClose={() => s.showUsageStats = false}
    />
  {/if}
  {#if settings.visible}
    <SettingsPanel
      onClose={closeSettingsPanel}
    />
  {/if}
</div>

<style>
  .input-area {
    position: relative;
  }
  .slash-wrapper {
    max-width: 640px;
    margin: 0 auto;
    padding: 0 24px;
    position: relative;
  }
  .scroll-to-bottom-btn {
    position: fixed;
    bottom: 5.5rem;
    right: 2rem;
    width: 2.25rem;
    height: 2.25rem;
    border-radius: 50%;
    border: 1px solid rgba(200, 168, 130, 0.25);
    background: rgba(8, 8, 8, 0.85);
    color: var(--color-amber);
    font-size: 1rem;
    cursor: pointer;
    display: flex;
    align-items: center;
    justify-content: center;
    transition: opacity 0.2s, border-color 0.2s;
    z-index: 10;
    animation: fade-in-simple 0.2s ease-out;
  }
  .scroll-to-bottom-btn:hover {
    border-color: var(--color-amber);
    opacity: 0.9;
  }
  .connection-lost-banner {
    background: #b91c1c;
    color: #fff;
    text-align: center;
    padding: 0.6em 1em;
    font-size: 0.85rem;
    letter-spacing: 0.02em;
  }
</style>
