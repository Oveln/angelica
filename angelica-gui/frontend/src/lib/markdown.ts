import { marked } from 'marked';
import { escapeHtml } from './html';
import hljs from 'highlight.js/lib/core';
import rust from 'highlight.js/lib/languages/rust';
import ts from 'highlight.js/lib/languages/typescript';
import js from 'highlight.js/lib/languages/javascript';
import python from 'highlight.js/lib/languages/python';
import go from 'highlight.js/lib/languages/go';
import bash from 'highlight.js/lib/languages/bash';
import jsonLang from 'highlight.js/lib/languages/json';
import xml from 'highlight.js/lib/languages/xml';
import css from 'highlight.js/lib/languages/css';
import ini from 'highlight.js/lib/languages/ini';
import yaml from 'highlight.js/lib/languages/yaml';
import sql from 'highlight.js/lib/languages/sql';

hljs.registerLanguage('rust', rust);
hljs.registerLanguage('typescript', ts);
hljs.registerLanguage('javascript', js);
hljs.registerLanguage('python', python);
hljs.registerLanguage('go', go);
hljs.registerLanguage('bash', bash);
hljs.registerLanguage('shell', bash);
hljs.registerLanguage('sh', bash);
hljs.registerLanguage('json', jsonLang);
hljs.registerLanguage('html', xml);
hljs.registerLanguage('xml', xml);
hljs.registerLanguage('css', css);
hljs.registerLanguage('toml', ini);
hljs.registerLanguage('ini', ini);
hljs.registerLanguage('yaml', yaml);
hljs.registerLanguage('sql', sql);

const renderer = new marked.Renderer();

renderer.code = function ({ text, lang }: { text: string; lang?: string }) {
  if (lang && hljs.getLanguage(lang)) {
    try {
      const highlighted = hljs.highlight(text, { language: lang }).value;
      return `<div class="code-block"><div class="code-lang">${escapeHtml(lang)}</div><pre><code class="hljs">${highlighted}</code></pre></div>`;
    } catch {
      // fall through
    }
  }
  return `<pre class="code-block"><code>${escapeHtml(text)}</code></pre>`;
};

marked.use({ renderer });

export function renderMarkdown(text: string): string {
  return marked.parse(text) as string;
}
