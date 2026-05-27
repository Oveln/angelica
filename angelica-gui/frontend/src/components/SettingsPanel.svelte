<script lang="ts">
  import { loadConfig, saveConfig, getDataDir } from '$lib/api';

  // ── Types ──

  interface ProviderEntry {
    name: string;
    adapter: string;
    model: string;
    base_url: string;
    api_key: string;
    max_tokens: string;
    temperature: string;
    thinking: boolean | '';
    reasoning_effort: string;
  }

  interface FormData {
    // LLM
    max_iterations: string;
    role_immersion: boolean | '';
    default_provider: string;
    providers: ProviderEntry[];
    // Memory
    episodes_path: string;
    self_path: string;
    profiles_dir: string;
    notebook_path: string;
    max_file_size_kb: string;
    recent_threshold: string;
    episode_inject_budget: string;
    recall_similarity_threshold: string;
    recall_inject_threshold: string;
    recall_inject_probability: string;
    self_hard_limit: string;
    profile_hard_limit: string;
    // Embedding
    embed_enabled: boolean;
    embed_model: string;
    embed_base_url: string;
    embed_api_key_env: string;
    // Fatigue
    max_context_tokens: string;
    curve_exponent: string;
    sleep_threshold: string;
    can_sleep_threshold: string;
    groggy_turns: string;
    // State
    state_path: string;
    conversation_path: string;
    // Skills
    skills_dir: string;
  }

  type TabId = 'llm' | 'memory' | 'embedding' | 'fatigue' | 'other';

  let {
    onClose,
  }: {
    onClose: () => void;
  } = $props();

  let activeTab = $state<TabId>('llm');
  let form = $state<FormData>(emptyForm());
  let originalToml = $state('');
  let dirty = $state(false);
  let saving = $state(false);
  let statusMsg = $state<{ text: string; error: boolean } | null>(null);
  let loading = $state(true);
  let errorMsg = $state('');
  let dataDir = $state('');
  let expandedProvider = $state(-1);

  const ADAPTERS = ['DeepSeek', 'OpenAI', 'Anthropic', 'Groq', 'Ollama', 'Gemini'];

  const TABS: { id: TabId; label: string }[] = [
    { id: 'llm', label: 'LLM' },
    { id: 'memory', label: '记忆' },
    { id: 'embedding', label: '嵌入' },
    { id: 'fatigue', label: '疲劳' },
    { id: 'other', label: '其他' },
  ];

  // ── Helpers ──

  function emptyForm(): FormData {
    return {
      max_iterations: '10', role_immersion: '', default_provider: '', providers: [],
      episodes_path: 'episodes.jsonl', self_path: 'SELF.md', profiles_dir: 'profiles',
      notebook_path: 'notebook.md', max_file_size_kb: '32', recent_threshold: '5',
      episode_inject_budget: '2', recall_similarity_threshold: '0.6',
      recall_inject_threshold: '0.7', recall_inject_probability: '0.6',
      self_hard_limit: '8192', profile_hard_limit: '8192',
      embed_enabled: true, embed_model: 'qwen3-embedding',
      embed_base_url: 'http://localhost:11434', embed_api_key_env: '',
      max_context_tokens: '120000', curve_exponent: '1.0',
      sleep_threshold: '0.85', can_sleep_threshold: '0.6', groggy_turns: '3',
      state_path: 'state.json', conversation_path: 'conversation.jsonl',
      skills_dir: 'skills',
    };
  }

  function optStr(v: any): string {
    return v == null ? '' : String(v);
  }

  function optBool(v: any): boolean | '' {
    return v == null ? '' : !!v;
  }

  function tomlToForm(raw: string): FormData {
    const p = parseTomlLoose(raw);
    const f = emptyForm();

    // LLM
    const llm = p.llm ?? {};
    f.max_iterations = optStr(llm.max_iterations ?? 10);
    f.role_immersion = optBool(llm.role_immersion);
    f.default_provider = optStr(llm.default_provider ?? '');
    f.providers = Array.isArray(llm.providers) ? llm.providers.map(providerToEntry) : [];

    // Memory
    const mem = p.memory ?? {};
    f.episodes_path = optStr(mem.episodes_path ?? 'episodes.jsonl');
    f.self_path = optStr(mem.self_path ?? 'SELF.md');
    f.profiles_dir = optStr(mem.profiles_dir ?? 'profiles');
    f.notebook_path = optStr(mem.notebook_path ?? 'notebook.md');
    f.max_file_size_kb = optStr(mem.max_file_size_kb ?? 32);
    f.recent_threshold = optStr(mem.recent_threshold ?? 5);
    f.episode_inject_budget = optStr(mem.episode_inject_budget ?? 2);
    f.recall_similarity_threshold = optStr(mem.recall_similarity_threshold ?? 0.6);
    f.recall_inject_threshold = optStr(mem.recall_inject_threshold ?? 0.7);
    f.recall_inject_probability = optStr(mem.recall_inject_probability ?? 0.6);
    f.self_hard_limit = optStr(mem.self_hard_limit ?? 8192);
    f.profile_hard_limit = optStr(mem.profile_hard_limit ?? 8192);

    // Embedding
    const emb = p.embedding ?? {};
    f.embed_enabled = emb.enabled !== false;
    f.embed_model = optStr(emb.model ?? 'qwen3-embedding');
    f.embed_base_url = optStr(emb.base_url ?? 'http://localhost:11434');
    f.embed_api_key_env = optStr(emb.api_key_env ?? '');

    // Fatigue
    const fat = p.fatigue ?? {};
    f.max_context_tokens = optStr(fat.max_context_tokens ?? 120000);
    f.curve_exponent = optStr(fat.curve_exponent ?? 1.0);
    f.sleep_threshold = optStr(fat.sleep_threshold ?? 0.85);
    f.can_sleep_threshold = optStr(fat.can_sleep_threshold ?? 0.6);
    f.groggy_turns = optStr(fat.groggy_turns ?? 3);

    // State
    const st = p.state ?? {};
    f.state_path = optStr(st.path ?? 'state.json');
    f.conversation_path = optStr(st.conversation_path ?? 'conversation.jsonl');

    // Skills
    const sk = p.skills ?? {};
    f.skills_dir = optStr(sk.directory ?? 'skills');

    return f;
  }

  function providerToEntry(p: any): ProviderEntry {
    return {
      name: optStr(p.name ?? ''),
      adapter: optStr(p.adapter ?? 'DeepSeek'),
      model: optStr(p.model ?? ''),
      base_url: optStr(p.base_url ?? ''),
      api_key: optStr(p.api_key ?? ''),
      max_tokens: optStr(p.max_tokens ?? ''),
      temperature: optStr(p.temperature ?? ''),
      thinking: optBool(p.thinking),
      reasoning_effort: optStr(p.reasoning_effort ?? ''),
    };
  }

  function formToToml(): string {
    const lines: string[] = [];

    // LLM
    lines.push('[llm]');
    lines.push(`max_iterations = ${form.max_iterations}`);
    if (form.role_immersion !== '') lines.push(`role_immersion = ${form.role_immersion}`);
    if (form.default_provider) lines.push(`default_provider = "${form.default_provider}"`);
    lines.push('');
    for (const prov of form.providers) {
      lines.push('[[llm.providers]]');
      lines.push(`name = "${prov.name}"`);
      lines.push(`adapter = "${prov.adapter}"`);
      if (prov.model) lines.push(`model = "${prov.model}"`);
      if (prov.base_url) lines.push(`base_url = "${prov.base_url}"`);
      if (prov.api_key) lines.push(`api_key = "${prov.api_key}"`);
      if (prov.max_tokens) lines.push(`max_tokens = ${prov.max_tokens}`);
      if (prov.temperature) lines.push(`temperature = ${prov.temperature}`);
      if (prov.thinking !== '') lines.push(`thinking = ${prov.thinking}`);
      if (prov.reasoning_effort) lines.push(`reasoning_effort = "${prov.reasoning_effort}"`);
      lines.push('');
    }

    // Memory
    lines.push('[memory]');
    lines.push(`episodes_path = "${form.episodes_path}"`);
    lines.push(`self_path = "${form.self_path}"`);
    lines.push(`profiles_dir = "${form.profiles_dir}"`);
    lines.push(`notebook_path = "${form.notebook_path}"`);
    lines.push(`max_file_size_kb = ${form.max_file_size_kb}`);
    lines.push(`recent_threshold = ${form.recent_threshold}`);
    lines.push(`episode_inject_budget = ${form.episode_inject_budget}`);
    lines.push(`recall_similarity_threshold = ${form.recall_similarity_threshold}`);
    lines.push(`recall_inject_threshold = ${form.recall_inject_threshold}`);
    lines.push(`recall_inject_probability = ${form.recall_inject_probability}`);
    lines.push(`self_hard_limit = ${form.self_hard_limit}`);
    lines.push(`profile_hard_limit = ${form.profile_hard_limit}`);
    lines.push('');

    // Embedding
    lines.push('[embedding]');
    lines.push(`enabled = ${form.embed_enabled}`);
    lines.push(`model = "${form.embed_model}"`);
    lines.push(`base_url = "${form.embed_base_url}"`);
    if (form.embed_api_key_env) lines.push(`api_key_env = "${form.embed_api_key_env}"`);
    lines.push('');

    // Fatigue
    lines.push('[fatigue]');
    lines.push(`max_context_tokens = ${form.max_context_tokens}`);
    lines.push(`curve_exponent = ${form.curve_exponent}`);
    lines.push(`sleep_threshold = ${form.sleep_threshold}`);
    lines.push(`can_sleep_threshold = ${form.can_sleep_threshold}`);
    lines.push(`groggy_turns = ${form.groggy_turns}`);
    lines.push('');

    // State
    lines.push('[state]');
    lines.push(`path = "${form.state_path}"`);
    lines.push(`conversation_path = "${form.conversation_path}"`);
    lines.push('');

    // Skills
    lines.push('[skills]');
    lines.push(`directory = "${form.skills_dir}"`);
    lines.push('');

    return lines.join('\n');
  }

  // Minimal TOML parser
  function parseTomlLoose(raw: string): Record<string, any> {
    const result: Record<string, any> = {};
    let currentObj: any = result;

    for (const line of raw.split('\n')) {
      const trimmed = line.trim();
      if (!trimmed || trimmed.startsWith('#')) continue;

      const arrayHeaderMatch = trimmed.match(/^\[\[(.+)\]\]$/);
      if (arrayHeaderMatch) {
        const path = arrayHeaderMatch[1];
        const parts = path.split('.');
        let parent: any = result;
        for (let i = 0; i < parts.length - 1; i++) {
          if (!parent[parts[i]]) parent[parts[i]] = {};
          parent = parent[parts[i]];
        }
        const lastKey = parts[parts.length - 1];
        if (!parent[lastKey]) parent[lastKey] = [];
        const newObj: Record<string, any> = {};
        parent[lastKey].push(newObj);
        currentObj = newObj;
        continue;
      }

      const headerMatch = trimmed.match(/^\[(.+)\]$/);
      if (headerMatch) {
        const section = headerMatch[1];
        const parts = section.split('.');
        let parent: any = result;
        for (const p of parts) {
          if (!parent[p]) parent[p] = {};
          parent = parent[p];
        }
        currentObj = parent;
        continue;
      }

      const kvMatch = trimmed.match(/^([\w.-]+)\s*=\s*(.+)$/);
      if (kvMatch) {
        currentObj[kvMatch[1]] = parseValue(kvMatch[2].trim());
      }
    }
    return result;
  }

  function parseValue(v: string): any {
    if ((v.startsWith('"') && v.endsWith('"')) || (v.startsWith("'") && v.endsWith("'"))) return v.slice(1, -1);
    if (v === 'true') return true;
    if (v === 'false') return false;
    const n = Number(v);
    if (!isNaN(n) && v !== '') return n;
    return v;
  }

  function markDirty() { dirty = true; }

  function addProvider() {
    form.providers.push({
      name: '', adapter: 'DeepSeek', model: '', base_url: '', api_key: '',
      max_tokens: '', temperature: '', thinking: '', reasoning_effort: '',
    });
    expandedProvider = form.providers.length - 1;
    markDirty();
  }

  function removeProvider(idx: number) {
    form.providers.splice(idx, 1);
    if (expandedProvider >= form.providers.length) expandedProvider = form.providers.length - 1;
    markDirty();
  }

  // ── Load / Save ──

  async function load() {
    try {
      originalToml = await loadConfig();
      form = tomlToForm(originalToml);
      loading = false;
      dataDir = await getDataDir().catch(() => '');
    } catch (e: any) {
      errorMsg = e?.toString() ?? 'Failed to load config';
      loading = false;
      dataDir = await getDataDir().catch(() => '');
    }
  }

  async function save() {
    saving = true;
    statusMsg = null;
    try {
      const tomlStr = formToToml();
      const msg = await saveConfig(tomlStr);
      statusMsg = { text: msg, error: false };
      dirty = false;
      originalToml = tomlStr;
    } catch (e: any) {
      statusMsg = { text: e?.toString() ?? 'Save failed', error: true };
    }
    saving = false;
  }

  function resetAll() {
    form = tomlToForm(originalToml);
    dirty = false;
    statusMsg = null;
  }

  function handleKeydown(e: KeyboardEvent) {
    if (e.target instanceof HTMLInputElement || e.target instanceof HTMLTextAreaElement || e.target instanceof HTMLSelectElement) return;
    if (e.key === 'Escape') { e.preventDefault(); onClose(); }
    if (e.key === 's' && (e.metaKey || e.ctrlKey)) { e.preventDefault(); save(); }
  }

  load();
</script>

<svelte:window onkeydown={handleKeydown} />

<div class="overlay" onclick={onClose} role="presentation">
  <!-- svelte-ignore a11y_click_events_have_key_events -->
  <div class="modal" onclick={(e) => e.stopPropagation()} role="dialog" tabindex="-1">
    <div class="modal-header">
      <h2 class="modal-title">设置{dirty ? ' ·' : ''}</h2>
      <button class="close-btn" onclick={onClose} title="关闭">&times;</button>
    </div>

    {#if loading}
      <div class="empty-state">
        <p>{errorMsg || 'Loading...'}</p>
      </div>
    {:else}
      <div class="tab-bar">
        {#each TABS as tab}
          <button
            class="tab-btn"
            class:active={activeTab === tab.id}
            onclick={() => activeTab = tab.id}
          >{tab.label}</button>
        {/each}
      </div>

      <div class="tab-content">
        {#if activeTab === 'llm'}
          <div class="form-group">
            <label class="field">
              <span class="label-text">最大迭代次数</span>
              <input type="number" class="input" bind:value={form.max_iterations} oninput={markDirty} />
            </label>
            <label class="field">
              <span class="label-text">默认 Provider</span>
              <input type="text" class="input" bind:value={form.default_provider} oninput={markDirty} placeholder="留空使用第一个" />
            </label>
            <label class="field field-row">
              <span class="label-text">角色沉浸模式</span>
              <div class="toggle-wrapper">
                <input type="checkbox" class="toggle" checked={form.role_immersion === true} onchange={() => { form.role_immersion = form.role_immersion === true ? '' : true; markDirty(); }} />
                <span class="toggle-label">{form.role_immersion === true ? '开' : '默认'}</span>
              </div>
            </label>
          </div>

          <div class="section-divider">
            <span class="section-label">Providers</span>
            <button class="icon-btn" onclick={addProvider} title="添加 Provider">+</button>
          </div>

          {#each form.providers as prov, i}
            <div class="provider-card" class:expanded={expandedProvider === i}>
              <!-- svelte-ignore a11y_click_events_have_key_events -->
              <!-- svelte-ignore a11y_no_static_element_interactions -->
              <div class="provider-header" role="button" tabindex="0" onclick={() => expandedProvider = expandedProvider === i ? -1 : i}>
                <span class="provider-name">{prov.name || '未命名'}</span>
                <span class="provider-meta">{prov.adapter}{prov.model ? ` · ${prov.model}` : ''}</span>
                <span class="provider-arrow">{expandedProvider === i ? '▾' : '▸'}</span>
              </div>
              {#if expandedProvider === i}
                <div class="provider-body">
                  <div class="form-group compact">
                    <label class="field">
                      <span class="label-text">名称</span>
                      <input type="text" class="input" bind:value={prov.name} oninput={markDirty} />
                    </label>
                    <label class="field">
                      <span class="label-text">适配器</span>
                      <select class="input select" bind:value={prov.adapter} onchange={markDirty}>
                        {#each ADAPTERS as a}
                          <option value={a}>{a}</option>
                        {/each}
                      </select>
                    </label>
                    <label class="field">
                      <span class="label-text">模型</span>
                      <input type="text" class="input" bind:value={prov.model} oninput={markDirty} />
                    </label>
                    <label class="field">
                      <span class="label-text">Base URL</span>
                      <input type="text" class="input" bind:value={prov.base_url} oninput={markDirty} placeholder="留空使用默认" />
                    </label>
                    <label class="field">
                      <span class="label-text">API Key</span>
                      <input type="password" class="input" bind:value={prov.api_key} oninput={markDirty} placeholder="留空从环境变量读取" />
                    </label>
                    <div class="field-row-2">
                      <label class="field">
                        <span class="label-text">max_tokens</span>
                        <input type="number" class="input" bind:value={prov.max_tokens} oninput={markDirty} placeholder="默认" />
                      </label>
                      <label class="field">
                        <span class="label-text">temperature</span>
                        <input type="number" step="0.1" class="input" bind:value={prov.temperature} oninput={markDirty} placeholder="默认" />
                      </label>
                    </div>
                    <div class="field-row-2">
                      <label class="field field-row">
                        <span class="label-text">Thinking</span>
                        <div class="toggle-wrapper">
                          <input type="checkbox" class="toggle" checked={prov.thinking === true} onchange={() => { prov.thinking = prov.thinking === true ? '' : true; markDirty(); }} />
                          <span class="toggle-label">{prov.thinking === true ? '开' : '默认'}</span>
                        </div>
                      </label>
                      <label class="field">
                        <span class="label-text">reasoning_effort</span>
                        <input type="text" class="input" bind:value={prov.reasoning_effort} oninput={markDirty} placeholder="默认" />
                      </label>
                    </div>
                    <button class="danger-btn" onclick={() => removeProvider(i)}>删除此 Provider</button>
                  </div>
                </div>
              {/if}
            </div>
          {/each}

          {#if form.providers.length === 0}
            <div class="empty-hint">暂无 Provider，点击上方 + 添加</div>
          {/if}

        {:else if activeTab === 'memory'}
          {#if dataDir}
            <div class="data-dir-hint compact">
              <code class="data-dir-path">{dataDir}</code>
              <span class="data-dir-note">以下路径相对于此目录</span>
            </div>
          {/if}
          <div class="form-group">
            <label class="field">
              <span class="label-text">情景记忆路径</span>
              <input type="text" class="input" bind:value={form.episodes_path} oninput={markDirty} />
            </label>
            <label class="field">
              <span class="label-text">SELF 路径</span>
              <input type="text" class="input" bind:value={form.self_path} oninput={markDirty} />
            </label>
            <label class="field">
              <span class="label-text">Profiles 目录</span>
              <input type="text" class="input" bind:value={form.profiles_dir} oninput={markDirty} />
            </label>
            <label class="field">
              <span class="label-text">Notebook 路径</span>
              <input type="text" class="input" bind:value={form.notebook_path} oninput={markDirty} />
            </label>
          </div>
          <div class="section-divider"><span class="section-label">召回参数</span></div>
          <div class="form-group">
            <div class="field-row-2">
              <label class="field">
                <span class="label-text">最大文件大小 (KB)</span>
                <input type="number" class="input" bind:value={form.max_file_size_kb} oninput={markDirty} />
              </label>
              <label class="field">
                <span class="label-text">近期阈值</span>
                <input type="number" class="input" bind:value={form.recent_threshold} oninput={markDirty} />
              </label>
            </div>
            <div class="field-row-2">
              <label class="field">
                <span class="label-text">注入预算</span>
                <input type="number" class="input" bind:value={form.episode_inject_budget} oninput={markDirty} />
              </label>
              <label class="field">
                <span class="label-text">召回相似度阈值</span>
                <input type="number" step="0.05" class="input" bind:value={form.recall_similarity_threshold} oninput={markDirty} />
              </label>
            </div>
            <div class="field-row-2">
              <label class="field">
                <span class="label-text">召回注入阈值</span>
                <input type="number" step="0.05" class="input" bind:value={form.recall_inject_threshold} oninput={markDirty} />
              </label>
              <label class="field">
                <span class="label-text">召回注入概率</span>
                <input type="number" step="0.05" class="input" bind:value={form.recall_inject_probability} oninput={markDirty} />
              </label>
            </div>
            <div class="field-row-2">
              <label class="field">
                <span class="label-text">SELF 硬限制</span>
                <input type="number" class="input" bind:value={form.self_hard_limit} oninput={markDirty} />
              </label>
              <label class="field">
                <span class="label-text">Profile 硬限制</span>
                <input type="number" class="input" bind:value={form.profile_hard_limit} oninput={markDirty} />
              </label>
            </div>
          </div>

        {:else if activeTab === 'embedding'}
          <div class="form-group">
            <label class="field field-row">
              <span class="label-text">启用嵌入</span>
              <div class="toggle-wrapper">
                <input type="checkbox" class="toggle" bind:checked={form.embed_enabled} onchange={markDirty} />
                <span class="toggle-label">{form.embed_enabled ? '开' : '关'}</span>
              </div>
            </label>
            <label class="field">
              <span class="label-text">嵌入模型</span>
              <input type="text" class="input" bind:value={form.embed_model} oninput={markDirty} />
            </label>
            <label class="field">
              <span class="label-text">Ollama / API 地址</span>
              <input type="text" class="input" bind:value={form.embed_base_url} oninput={markDirty} />
            </label>
            <label class="field">
              <span class="label-text">API Key 环境变量名</span>
              <input type="text" class="input" bind:value={form.embed_api_key_env} oninput={markDirty} placeholder="留空使用默认" />
            </label>
          </div>

        {:else if activeTab === 'fatigue'}
          <div class="form-group">
            <label class="field">
              <span class="label-text">最大上下文 Token</span>
              <input type="number" class="input" bind:value={form.max_context_tokens} oninput={markDirty} />
            </label>
            <label class="field">
              <span class="label-text">曲线指数</span>
              <input type="number" step="0.1" class="input" bind:value={form.curve_exponent} oninput={markDirty} />
            </label>
            <label class="field">
              <span class="label-text">入睡阈值</span>
              <input type="number" step="0.05" class="input" bind:value={form.sleep_threshold} oninput={markDirty} />
            </label>
            <label class="field">
              <span class="label-text">可入睡阈值</span>
              <input type="number" step="0.05" class="input" bind:value={form.can_sleep_threshold} oninput={markDirty} />
            </label>
            <label class="field">
              <span class="label-text">迷糊回合数</span>
              <input type="number" class="input" bind:value={form.groggy_turns} oninput={markDirty} />
            </label>
          </div>

        {:else if activeTab === 'other'}
          {#if dataDir}
            <div class="data-dir-hint">
              <span class="data-dir-label">数据根目录</span>
              <code class="data-dir-path">{dataDir}</code>
              <span class="data-dir-note">相对路径均以此目录为根</span>
            </div>
          {/if}
          <div class="form-group">
            <div class="sub-label">状态</div>
            <label class="field">
              <span class="label-text">状态文件路径</span>
              <input type="text" class="input" bind:value={form.state_path} oninput={markDirty} />
            </label>
            <label class="field">
              <span class="label-text">对话记录路径</span>
              <input type="text" class="input" bind:value={form.conversation_path} oninput={markDirty} />
            </label>
          </div>
          <div class="form-group">
            <div class="sub-label">技能</div>
            <label class="field">
              <span class="label-text">技能目录</span>
              <input type="text" class="input" bind:value={form.skills_dir} oninput={markDirty} />
            </label>
          </div>
        {/if}
      </div>

      {#if statusMsg}
        <div class="status-bar" class:error={statusMsg.error}>
          {statusMsg.text}
        </div>
      {/if}

      <div class="modal-footer">
        <span class="hint">保存后需重启生效 · Esc 关闭</span>
        <div class="footer-actions">
          {#if dirty}
            <button class="btn secondary" onclick={resetAll}>重置</button>
            <button class="btn primary" onclick={save} disabled={saving}>
              {saving ? '保存中...' : '保存'}
            </button>
          {/if}
        </div>
      </div>
    {/if}
  </div>
</div>

<style>
  .overlay {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.65);
    display: flex;
    align-items: center;
    justify-content: center;
    z-index: 100;
    animation: fade-in-simple 0.2s ease-out;
  }

  .modal {
    background: #0a0a0a;
    border: 1px solid #1e1e1e;
    border-radius: 12px;
    width: 94vw;
    max-width: 580px;
    max-height: 85vh;
    display: flex;
    flex-direction: column;
    overflow: hidden;
    box-shadow: 0 16px 64px rgba(0, 0, 0, 0.5);
  }

  .modal-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 14px 18px;
    border-bottom: 1px solid #1a1a1a;
  }

  .modal-title {
    font-size: 0.85rem;
    font-weight: 500;
    color: var(--color-amber);
    letter-spacing: 0.1em;
    margin: 0;
  }

  .close-btn {
    font-size: 1.1rem;
    color: var(--color-ink-dark);
    background: none;
    border: none;
    cursor: pointer;
    padding: 2px 6px;
    transition: color 0.2s;
  }
  .close-btn:hover { color: var(--color-ink-light); }

  .empty-state {
    padding: 40px 24px;
    text-align: center;
    color: var(--color-ink-faint);
    font-size: 0.85rem;
  }

  /* ── Tabs ── */

  .tab-bar {
    display: flex;
    gap: 0;
    border-bottom: 1px solid #1a1a1a;
    padding: 0 12px;
  }

  .tab-btn {
    padding: 8px 14px;
    font-size: 0.75rem;
    font-weight: 500;
    color: var(--color-ink-dark);
    background: none;
    border: none;
    border-bottom: 2px solid transparent;
    cursor: pointer;
    transition: all 0.15s;
    letter-spacing: 0.05em;
  }
  .tab-btn:hover { color: var(--color-ink-faint); }
  .tab-btn.active {
    color: var(--color-amber);
    border-bottom-color: var(--color-amber);
  }

  /* ── Content ── */

  .tab-content {
    flex: 1;
    overflow-y: auto;
    padding: 16px 18px;
  }

  .form-group {
    display: flex;
    flex-direction: column;
    gap: 12px;
    margin-bottom: 16px;
  }
  .form-group.compact { gap: 10px; }

  .field {
    display: flex;
    flex-direction: column;
    gap: 4px;
  }

  .label-text {
    font-size: 0.7rem;
    color: var(--color-ink-dark);
    letter-spacing: 0.04em;
  }

  .input {
    background: #0f0f0f;
    border: 1px solid #222;
    border-radius: 6px;
    color: var(--color-ink);
    font-family: var(--font-mono);
    font-size: 0.8rem;
    padding: 6px 10px;
    outline: none;
    transition: border-color 0.15s;
    width: 100%;
    box-sizing: border-box;
  }
  .input:focus { border-color: var(--color-amber-muted); }
  .input::placeholder { color: #333; }

  .field-row {
    flex-direction: row;
    align-items: center;
    justify-content: space-between;
  }

  .field-row-2 {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 12px;
  }

  .toggle-wrapper {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .toggle {
    accent-color: var(--color-amber);
    width: 14px;
    height: 14px;
  }

  .toggle-label {
    font-size: 0.7rem;
    color: var(--color-ink-faint);
  }

  .sub-label {
    font-size: 0.65rem;
    color: var(--color-ink-faint);
    text-transform: uppercase;
    letter-spacing: 0.08em;
    margin-bottom: -4px;
  }

  /* ── Provider ── */

  .section-divider {
    display: flex;
    align-items: center;
    justify-content: space-between;
    margin: 16px 0 10px;
    padding-top: 12px;
    border-top: 1px solid #1a1a1a;
  }

  .section-label {
    font-size: 0.65rem;
    color: var(--color-ink-faint);
    text-transform: uppercase;
    letter-spacing: 0.08em;
  }

  .icon-btn {
    width: 24px;
    height: 24px;
    display: flex;
    align-items: center;
    justify-content: center;
    background: rgba(200, 168, 130, 0.08);
    border: 1px solid #222;
    border-radius: 6px;
    color: var(--color-amber);
    font-size: 0.85rem;
    cursor: pointer;
    transition: all 0.15s;
  }
  .icon-btn:hover { background: rgba(200, 168, 130, 0.15); }

  .provider-card {
    background: #0c0c0c;
    border: 1px solid #1a1a1a;
    border-radius: 8px;
    margin-bottom: 8px;
    overflow: hidden;
  }
  .provider-card.expanded { border-color: #252525; }

  .provider-header {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 10px 14px;
    cursor: pointer;
    transition: background 0.1s;
  }
  .provider-header:hover { background: rgba(200, 168, 130, 0.03); }

  .provider-name {
    font-size: 0.8rem;
    font-weight: 500;
    color: var(--color-ink);
    min-width: 60px;
  }

  .provider-meta {
    font-size: 0.7rem;
    color: var(--color-ink-dark);
    font-family: var(--font-mono);
    flex: 1;
  }

  .provider-arrow {
    font-size: 0.7rem;
    color: var(--color-ink-dark);
  }

  .provider-body {
    padding: 4px 14px 14px;
    border-top: 1px solid #1a1a1a;
  }

  .danger-btn {
    margin-top: 4px;
    padding: 5px 12px;
    font-size: 0.7rem;
    color: #c06060;
    background: rgba(192, 80, 80, 0.08);
    border: 1px solid rgba(192, 80, 80, 0.2);
    border-radius: 6px;
    cursor: pointer;
    transition: all 0.15s;
  }
  .danger-btn:hover { background: rgba(192, 80, 80, 0.15); }

  .empty-hint {
    text-align: center;
    padding: 20px;
    font-size: 0.75rem;
    color: var(--color-ink-dark);
  }

  /* ── Footer ── */

  .status-bar {
    padding: 6px 18px;
    font-size: 0.7rem;
    border-top: 1px solid #141414;
  }
  .status-bar.error { color: #e06060; }
  .status-bar:not(.error) { color: #6a9e5f; }

  .modal-footer {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 10px 18px;
    border-top: 1px solid #1a1a1a;
  }

  .hint {
    font-size: 0.6rem;
    color: var(--color-ink-dark);
    letter-spacing: 0.04em;
  }
  /* ── Data Dir Hint ── */

  .data-dir-hint {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 6px 10px;
    background: rgba(200, 168, 130, 0.04);
    border: 1px solid #1a1a1a;
    border-radius: 6px;
    margin-bottom: 12px;
    flex-wrap: wrap;
  }
  .data-dir-hint.compact {
    padding: 4px 8px;
    margin-bottom: 8px;
    font-size: 0.65rem;
  }

  .data-dir-label {
    font-size: 0.6rem;
    color: var(--color-amber);
    text-transform: uppercase;
    letter-spacing: 0.06em;
    flex-shrink: 0;
  }

  .data-dir-path {
    font-family: var(--font-mono);
    font-size: 0.65rem;
    color: var(--color-ink-faint);
    background: #0f0f0f;
    padding: 1px 6px;
    border-radius: 3px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    max-width: 340px;
  }

  .data-dir-note {
    font-size: 0.6rem;
    color: var(--color-ink-dark);
    flex-shrink: 0;
  }

  .footer-actions {
    display: flex;
    gap: 8px;
  }

  .btn {
    padding: 5px 16px;
    font-size: 0.72rem;
    border-radius: 6px;
    border: 1px solid;
    cursor: pointer;
    transition: all 0.15s;
    font-family: var(--font-serif);
  }

  .btn.primary {
    color: #0a0a0a;
    background: var(--color-amber);
    border-color: var(--color-amber);
  }
  .btn.primary:hover { background: #d4b090; }
  .btn.primary:disabled { opacity: 0.5; cursor: default; }

  .btn.secondary {
    color: var(--color-ink-faint);
    background: transparent;
    border-color: #333;
  }
  .btn.secondary:hover { border-color: #444; color: var(--color-ink-light); }
</style>
