export interface Message {
  id: string;
  role: 'user' | 'assistant' | 'system';
  content: string;
  thinking: string;
  toolCalls: ToolCallInfo[];
  timestamp: number;
  done: boolean;
}

export interface ToolCallInfo {
  callId: string;
  name: string;
  arguments: string;
  result?: string;
  diffPreview?: string | null;
  pending?: boolean;
}

let nextId = 0;
export function genId(): string {
  return `msg-${++nextId}-${Date.now()}`;
}
