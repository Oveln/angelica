import { listen } from '@tauri-apps/api/event';
import { invoke } from '@tauri-apps/api/core';

export interface ThinkingDelta { delta: string }
export interface TextDelta { delta: string }
export interface TextDone { full_text: string }
export interface TurnComplete {}

export interface ToolCalling {
  call_id: string;
  name: string;
  display: string;
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
  tool_label: string;
  is_diff: boolean;
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

export interface UsageMetrics {
  prompt_tokens: number;
  completion_tokens: number;
  total_tokens: number;
  reasoning_tokens: number;
  cache_hit_tokens: number;
  cache_miss_tokens: number;
}

export interface UsageUpdate {
  record: UsageMetrics;
}

export interface SessionUsage {
  scope: 'awake' | 'sleep';
  start_time: string;
  iterations: number;
  prompt_tokens: number;
  completion_tokens: number;
  total_tokens: number;
  reasoning_tokens: number;
  cache_hit_tokens: number;
  cache_miss_tokens: number;
}

export interface InitEvent {
  entries: DisplayEntry[];
  current_usage: UsageMetrics | null;
  model_name: string;
}

export type DisplayEntry =
  | { type: 'chat'; role: 'user' | 'assistant' | 'system'; content: string; thinking: string | null }
  | { type: 'tool'; call_id: string; name: string; args_display: string; result: string | null; diff_preview: string | null };

export function requestInit(): Promise<void> {
  return invoke('request_init');
}

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

export function rebuildEmbeddings(): Promise<void> {
  return invoke('rebuild_embeddings');
}

export function requestUsageStats(): Promise<void> {
  return invoke('request_usage_stats');
}

export function quit(): Promise<void> {
  return invoke('quit');
}

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
  'usage-stats-loaded': { sessions: SessionUsage[] };
  'falling-asleep': {};
  'sleeping': {};
  'waking-up': { dream: string };
};

export function onAppEvent<K extends keyof AppEventMap>(
  event: K,
  handler: (payload: AppEventMap[K]) => void,
): Promise<() => void> {
  return listen<AppEventMap[K]>(event, (e) => handler(e.payload));
}
