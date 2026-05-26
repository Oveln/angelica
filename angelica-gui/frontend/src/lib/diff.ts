import { escapeHtml } from './html';

export function renderDiff(text: string): string {
  const lines = text.split('\n');
  return lines
    .map((line) => {
      const escaped = escapeHtml(line);
      if (line.startsWith('+++')) {
        return `<span class="diff-header">${escaped}</span>`;
      }
      if (line.startsWith('---')) {
        return `<span class="diff-header">${escaped}</span>`;
      }
      if (line.startsWith('@@')) {
        return `<span class="diff-hunk">${escaped}</span>`;
      }
      if (line.startsWith('+')) {
        return `<span class="diff-add">${escaped}</span>`;
      }
      if (line.startsWith('-')) {
        return `<span class="diff-del">${escaped}</span>`;
      }
      return `<span class="diff-ctx">${escaped}</span>`;
    })
    .join('\n');
}

export function isDiffContent(text: string): boolean {
  const first = text.split('\n')[0] ?? '';
  return first.startsWith('---') || first.startsWith('diff ');
}
