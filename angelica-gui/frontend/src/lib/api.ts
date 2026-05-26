import { listen } from '@tauri-apps/api/event';
import { invoke } from '@tauri-apps/api/core';
import type {
  ApprovalPendingPayload,
  ErrorPayload,
  FatigueUpdatePayload,
  InitPayload,
  ThinkingDeltaPayload,
  TextDeltaPayload,
  TextDonePayload,
  ToolCallingPayload,
  ToolResultPayload,
  ToolRejectedPayload,
  UsageUpdatePayload,
  UsageStatsLoadedPayload,
  WakingUpPayload,
  UsageMetrics,
  SessionUsage,
  DisplayEntry,
} from './api-generated';

// Convenience aliases matching the original shorter names
export type ApprovalPending = ApprovalPendingPayload;
export type ErrorEvent = ErrorPayload;
export type FatigueUpdate = FatigueUpdatePayload;
export type InitEvent = InitPayload;
export type ThinkingDelta = ThinkingDeltaPayload;
export type TextDelta = TextDeltaPayload;
export type TextDone = TextDonePayload;
export type ToolCalling = ToolCallingPayload;
export type ToolResult = ToolResultPayload;
export type ToolRejected = ToolRejectedPayload;
export type UsageUpdate = UsageUpdatePayload;

export type { UsageMetrics, SessionUsage, DisplayEntry };

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
  'init': InitPayload;
  'thinking-delta': ThinkingDeltaPayload;
  'text-delta': TextDeltaPayload;
  'text-done': TextDonePayload;
  'turn-complete': {};
  'tool-calling': ToolCallingPayload;
  'tool-result': ToolResultPayload;
  'approval-pending': ApprovalPendingPayload;
  'tool-rejected': ToolRejectedPayload;
  'error': ErrorPayload;
  'fatigue-update': FatigueUpdatePayload;
  'usage-update': UsageUpdatePayload;
  'usage-stats-loaded': UsageStatsLoadedPayload;
  'falling-asleep': {};
  'sleeping': {};
  'waking-up': WakingUpPayload;
};

export function onAppEvent<K extends keyof AppEventMap>(
  event: K,
  handler: (payload: AppEventMap[K]) => void,
): Promise<() => void> {
  return listen<AppEventMap[K]>(event, (e) => handler(e.payload));
}
