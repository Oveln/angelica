export interface ChatMessage {
  type: 'chat';
  id: string;
  role: 'user' | 'assistant' | 'system';
  content: string;
  thinking: string;
  timestamp: number;
}

export interface ToolMessage {
  type: 'tool';
  id: string;
  callId: string;
  name: string;
  display: string;
  result?: string;
  diffPreview?: string | null;
  pending: boolean;
  timestamp: number;
}

export type Message = ChatMessage | ToolMessage;

let nextId = 0;
export function genId(): string {
  return `msg-${++nextId}-${Date.now()}`;
}
