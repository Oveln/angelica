import { listen } from '@tauri-apps/api/event';
import { invoke } from '@tauri-apps/api/core';

// --- Events from backend (AppEvent) ---

export interface ThinkingDelta { delta: string }
export interface TextDelta { delta: string }
export interface TextDone { full_text: string }
export interface TurnComplete {}

export interface ToolCalling {
  call_id: string;
  name: string;
  arguments: string;
}

export interface ToolResult {
  call_id: string;
  name: string;
  result: string;
  diff_preview: string | null;
}

export interface ApprovalPending {
  call_id: string;
  tool_name: string;
  tool_target: string | null;
  preview: string;
}

export interface ToolRejected {
  call_id: string;
  feedback: string;
}

export interface ErrorEvent { message: string }

export interface FatigueUpdate {
  fatigue: number;
  turns: number;
  tool_calls: number;
  desc: string;
}

export interface UsageUpdate {
  record: unknown;
}

export interface InitMessage {
  role: string;
  content: string | null;
  context?: {
    time: string;
    fatigue?: string | null;
    turns: number;
    tool_calls: number;
    has_dream: boolean;
    recall?: string | null;
  } | null;
  reasoning_content?: string | null;
  tool_calls?: Array<{
    id: string;
    function: { name: string; arguments: string };
  }> | null;
  tool_call_id?: string | null;
  name?: string | null;
}

export interface InitEvent { messages: InitMessage[] }

export type AppEventMap = {
  'init': InitEvent;
  'thinking-delta': ThinkingDelta;
  'text-delta': TextDelta;
  'text-done': TextDone;
  'turn-complete': TurnComplete;
  'tool-calling': ToolCalling;
  'tool-result': ToolResult;
  'approval-pending': ApprovalPending;
  'tool-rejected': ToolRejected;
  'error': ErrorEvent;
  'fatigue-update': FatigueUpdate;
  'usage-update': UsageUpdate;
  'falling-asleep': {};
  'sleeping': {};
  'waking-up': { dream: string };
};

// --- Commands to backend (UserAction) ---

export function sendMessage(content: string): Promise<void> {
  return invoke('send_message', { content });
}

export function approvePending(): Promise<void> {
  return invoke('approve_pending');
}

export function approveAlways(tool: string, target: string, persist: boolean): Promise<void> {
  return invoke('approve_always', { tool, target, persist });
}

export function rejectTool(feedback?: string): Promise<void> {
  return invoke('reject_tool', { feedback: feedback ?? null });
}

export function forceSleep(): Promise<void> {
  return invoke('force_sleep');
}

export function quit(): Promise<void> {
  return invoke('quit');
}

export function getInitMessages(): Promise<InitMessage[] | null> {
  return invoke('get_init_messages');
}

// --- Typed event listener ---

export function onAppEvent<K extends keyof AppEventMap>(
  event: K,
  handler: (payload: AppEventMap[K]) => void,
): Promise<() => void> {
  return listen<AppEventMap[K]>(event, (e) => handler(e.payload));
}
