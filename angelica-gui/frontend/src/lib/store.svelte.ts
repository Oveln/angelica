import {
  onAppEvent,
  requestInit,
  type ApprovalPending as ApprovalPayload,
  type ToolCalling,
  type ToolResult,
  type FatigueUpdate,
  type UsageUpdate,
  type UsageMetrics,
  type SessionUsage,
  type DisplayEntry,
  type InitEvent,
} from '$lib/api';
import type { ChatMessage, ToolMessage, Message } from '$lib/types';
import { genId } from '$lib/types';

let messages = $state<Message[]>([]);
let thinkingBuffer = $state('');
let textBuffer = $state('');
let isStreaming = $state(false);
let fatigue = $state(0);
let fatigueDesc = $state('');
let fatigueTurns = $state(0);
let fatigueToolCalls = $state(0);
let statusText = $state('');
let approval = $state<ApprovalPayload | null>(null);
let modelName = $state('');
let thinkingVisible = $state(true);
let showUsageStats = $state(false);
let usageSessions = $state<SessionUsage[]>([]);
let usage = $state<UsageMetrics | null>(null);

const unsubs: (() => void)[] = [];

function commitBuffers() {
  if (!textBuffer && !thinkingBuffer) return;
  messages.push({
    type: 'chat',
    id: genId(),
    role: 'assistant',
    content: textBuffer,
    thinking: thinkingBuffer,
    timestamp: Date.now(),
  });
  thinkingBuffer = '';
  textBuffer = '';
}

function addSystemMessage(content: string) {
  messages.push({
    type: 'chat',
    id: genId(),
    role: 'system',
    content,
    thinking: '',
    timestamp: Date.now(),
  });
}

function addUserMessage(content: string) {
  messages.push({
    type: 'chat',
    id: genId(),
    role: 'user',
    content,
    thinking: '',
    timestamp: Date.now(),
  });
}

function convertEntries(entries: DisplayEntry[]): Message[] {
  const results: Message[] = [];
  for (const entry of entries) {
    if (entry.type === 'chat') {
      results.push({
        type: 'chat',
        id: genId(),
        role: entry.role,
        content: entry.content ?? '',
        thinking: entry.thinking ?? '',
        timestamp: Date.now(),
      });
    } else {
      results.push({
        type: 'tool',
        id: genId(),
        callId: entry.call_id,
        name: entry.name,
        display: entry.args_display,
        result: entry.result ?? undefined,
        diffPreview: entry.diff_preview,
        pending: !entry.result,
        timestamp: Date.now(),
      });
    }
  }
  return results;
}

async function listenTo<K extends keyof import('$lib/api').AppEventMap>(
  event: K,
  handler: (payload: import('$lib/api').AppEventMap[K]) => void,
) {
  const unsub = await onAppEvent(event, handler);
  unsubs.push(unsub);
}

async function init() {
  await listenTo('init', (p: InitEvent) => {
    messages = convertEntries(p.entries);
    if (p.current_usage) usage = p.current_usage;
    if (p.model_name) modelName = p.model_name;
  });

  await listenTo('thinking-delta', (p) => {
    thinkingBuffer += p.delta;
    isStreaming = true;
  });

  await listenTo('text-delta', (p) => {
    textBuffer += p.delta;
    isStreaming = true;
  });

  await listenTo('text-done', (p) => {
    const thinking = thinkingBuffer || null;
    thinkingBuffer = '';
    textBuffer = '';
    messages.push({
      type: 'chat',
      id: genId(),
      role: 'assistant',
      content: p.full_text,
      thinking: thinking || '',
      timestamp: Date.now(),
    });
  });

  await listenTo('turn-complete', () => {
    commitBuffers();
    isStreaming = false;
  });

  await listenTo('tool-calling', (p: ToolCalling) => {
    if (textBuffer || thinkingBuffer) commitBuffers();
    messages.push({
      type: 'tool',
      id: genId(),
      callId: p.call_id,
      name: p.name,
      display: p.display,
      result: undefined,
      diffPreview: null,
      pending: true,
      timestamp: Date.now(),
    });
  });

  await listenTo('tool-result', (p: ToolResult) => {
    for (let i = messages.length - 1; i >= 0; i--) {
      const m = messages[i];
      if (m.type === 'tool' && m.callId === p.call_id) {
        m.result = p.result;
        m.diffPreview = p.diff_preview;
        m.pending = false;
        return;
      }
    }
  });

  await listenTo('approval-pending', (p: ApprovalPayload) => {
    approval = p;
  });

  await listenTo('tool-rejected', (p) => {
    approval = null;
    for (let i = messages.length - 1; i >= 0; i--) {
      const m = messages[i];
      if (m.type === 'tool' && m.callId === p.call_id) {
        m.result = p.feedback || '已拒绝';
        m.pending = false;
        return;
      }
    }
  });

  await listenTo('error', (p) => {
    commitBuffers();
    isStreaming = false;
    addSystemMessage(`Error: ${p.message}`);
  });

  await listenTo('fatigue-update', (p: FatigueUpdate) => {
    fatigue = p.fatigue;
    fatigueDesc = p.desc;
    fatigueTurns = p.turns;
    fatigueToolCalls = p.tool_calls;
    statusText = p.desc;
  });

  await listenTo('usage-update', (p: UsageUpdate) => {
    usage = p.metrics;
  });

  await listenTo('usage-stats-loaded', (p) => {
    usageSessions = p.sessions;
    showUsageStats = true;
  });

  await listenTo('falling-asleep', () => {
    commitBuffers();
    isStreaming = false;
    addSystemMessage('祈芷正在沉睡...');
    usage = null;
  });

  await listenTo('sleeping', () => {
    isStreaming = false;
    statusText = '沉睡中';
  });

  await listenTo('waking-up', () => {
    addSystemMessage('祈芷醒来了，梦的余韵还留在心头。');
  });

  await listenTo('undo-done', (p: InitEvent) => {
    messages = convertEntries(p.entries);
    addSystemMessage('已撤回。');
  });

  try {
    await requestInit();
  } catch (e) {
    console.error('Failed to request init:', e);
  }
}

function destroy() {
  for (const fn of unsubs) fn();
}

function clearApproval() {
  approval = null;
}

export function getStore() {
  return {
    get messages() { return messages; },
    get thinkingBuffer() { return thinkingBuffer; },
    get textBuffer() { return textBuffer; },
    get isStreaming() { return isStreaming; },
    get fatigue() { return fatigue; },
    get fatigueDesc() { return fatigueDesc; },
    get fatigueTurns() { return fatigueTurns; },
    get fatigueToolCalls() { return fatigueToolCalls; },
    get statusText() { return statusText; },
    get approval() { return approval; },
    get modelName() { return modelName; },
    get thinkingVisible() { return thinkingVisible; },
    get showUsageStats() { return showUsageStats; },
    get usageSessions() { return usageSessions; },
    get usage() { return usage; },
    get inputDisabled() { return isStreaming || approval !== null; },
    set thinkingVisible(v: boolean) { thinkingVisible = v; },
    set showUsageStats(v: boolean) { showUsageStats = v; },
    set approval(v: ApprovalPayload | null) { approval = v; },
    addSystemMessage,
    addUserMessage,
    clearApproval,
    init,
    destroy,
  };
}
