import {
  quit as apiQuit,
  forceSleep as apiForceSleep,
  rebuildEmbeddings as apiRebuildEmbeddings,
  requestUsageStats as apiRequestUsageStats,
  undo as apiUndo,
} from '$lib/api';
import { getStore } from '$lib/store.svelte';

// Settings panel state: use a reactive object so consumers can bind
const settingsState = $state({ visible: false });
export function showSettingsPanel() { return settingsState; }
export function openSettingsPanel() { settingsState.visible = true; }
export function closeSettingsPanel() { settingsState.visible = false; }

export interface SlashCommand {
  name: string;
  aliases: string[];
  description: string;
}

export const BUILTIN_COMMANDS: SlashCommand[] = [
  { name: 'help', aliases: ['?'], description: '显示可用命令' },
  { name: 'quit', aliases: ['q'], description: '退出应用' },
  { name: 'thinking', aliases: ['think'], description: '切换思考过程显示' },
  { name: 'model', aliases: [], description: '显示当前模型' },
  { name: 'history', aliases: ['h'], description: '显示消息统计' },
  { name: 'sleep', aliases: [], description: '让祈芷入睡（梦境与沉淀）' },
  { name: 'rebuild-embeddings', aliases: ['rebuild'], description: '重建情景记忆的嵌入向量' },
  { name: 'usage', aliases: ['stats'], description: '显示 token 用量统计' },
  { name: 'settings', aliases: ['set', 'config'], description: '打开设置面板' },
  { name: 'undo', aliases: ['u'], description: '撤回上一条消息' },
];

export async function executeSlashCommand(cmd: string): Promise<void> {
  const s = getStore();
  const cmdName = cmd.split(' ')[0].toLowerCase();
  const matched = BUILTIN_COMMANDS.find(
    (c) => c.name === cmdName || c.aliases.some((a) => a === cmdName)
  );

  if (!matched) {
    s.addSystemMessage(`未知命令: /${cmdName}。输入 /help 查看可用命令。`);
    return;
  }

  switch (matched.name) {
    case 'help': {
      let help = '可用命令:\n';
      for (const c of BUILTIN_COMMANDS) {
        const aliases = c.aliases.length > 0 ? ` (${c.aliases.join(', ')})` : '';
        help += `  /${c.name}${aliases}\n    ${c.description}\n`;
      }
      s.addSystemMessage(help);
      break;
    }
    case 'quit':
      await apiQuit();
      break;
    case 'thinking': {
      s.thinkingVisible = !s.thinkingVisible;
      s.addSystemMessage(`思考过程显示: ${s.thinkingVisible ? '开' : '关'}`);
      break;
    }
    case 'model':
      s.addSystemMessage(s.modelName || '未知');
      break;
    case 'history': {
      const userCount = s.messages.filter((m) => m.type === 'chat' && m.role === 'user').length;
      s.addSystemMessage(`${s.messages.length} 条消息（${userCount} 条用户）`);
      break;
    }
    case 'sleep':
      s.addSystemMessage('正在入睡...');
      await apiForceSleep();
      break;
    case 'rebuild-embeddings':
      s.addSystemMessage('正在重建嵌入向量...');
      await apiRebuildEmbeddings();
      break;
    case 'usage':
      await apiRequestUsageStats();
      break;
    case 'settings':
      openSettingsPanel();
      break;
    case 'undo':
      s.addSystemMessage('正在撤回...');
      await apiUndo();
      break;
  }
}
