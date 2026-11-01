// The AI assistant: a floating window that chats with a model on the user's own PC and can use
// the app's tools (read logs and configs, change settings, run commands…) with their approval.
import { useEffect, useRef, useState, type PointerEvent as RPointerEvent } from 'react';
import { AnimatePresence, motion } from 'motion/react';
import { Bot, Check, ChevronDown, ChevronRight, Loader2, Maximize2, Minimize2, Plus, Send, Settings2, ShieldAlert, Square, Wrench, X } from 'lucide-react';
import { api, errMsg } from '@/lib/ipc';
import { useApp } from '@/store/app';
import { cn } from '@/lib/cn';
import { Button, IconButton } from '@/components/ui/Button';
import { Input, SecretInput, Segmented, Toggle } from '@/components/ui/Field';
import { Markdown } from '@/components/ui/Markdown';
import type { AiEvent, AiSettings, AiToolCall } from '@/lib/types';
import { parseArgs, resultText, TOOL_MAP, toolSpecs } from './tools';
import { systemPrompt } from './prompt';

const PRESETS: [string, string, string][] = [
  ['Ollama', 'http://localhost:11434/v1', 'ollama serve, then e.g. ollama pull qwen3:8b'],
  ['LM Studio', 'http://localhost:1234/v1', 'Developer tab → Start server'],
  ['llama.cpp', 'http://localhost:8080/v1', 'llama-server -m model.gguf --jinja'],
  ['Jan', 'http://localhost:1337/v1', 'Settings → Local API server'],
];

const ASKS: [string, string][] = [
  ['Why did it crash?', 'My server crashed or will not start. Read the console and crash reports, find the cause and fix it.'],
  ['Health check', 'Run the health check on my server and explain anything that needs attention, then offer to fix it.'],
  ['Make it faster', 'My server lags. Check TPS, settings, memory and mods and suggest (or apply) the best performance improvements.'],
  ['Friends can’t join', 'My friends cannot join from outside my home network. Find out why and help me fix it.'],
  ['Nightly restart + backups', 'Set up a daily 4:00 restart with in-game warnings and a backup every 2 hours (keep 12).'],
  ['Recommend mods', 'Recommend useful server-side mods or plugins for my server and offer to install them.'],
];

const MAX_ROUNDS = 16;
/** "I've updated / changed / installed…" – an answer that claims to have done something. */
const CLAIM = /\b(I('ve| have)?|has been|have been|was|were)\s+(now\s+)?(updated|changed|set|installed|enabled|disabled|removed|created|started|stopped|restarted|edited|written|added|applied|saved)\b/i;

type Item =
  | { kind: 'user'; text: string }
  | { kind: 'assistant'; text: string; thinking: string; streaming: boolean }
  | { kind: 'tool'; name: string; label: string; status: 'waiting' | 'running' | 'ok' | 'error' | 'declined'; result?: string }
  | { kind: 'notice'; text: string; tone: 'info' | 'error' };

type Approval = { label: string; resolve: (v: 'yes' | 'all' | 'no') => void };

const GEOM_KEY = 'mcsm.ai.window';

function loadGeom() {
  try {
    const g = JSON.parse(localStorage.getItem(GEOM_KEY) ?? 'null');
    if (g && typeof g.w === 'number') return g as { x: number; y: number; w: number; h: number };
  } catch {
    /* default below */
  }
  return { x: Math.max(16, window.innerWidth - 500), y: 64, w: 460, h: Math.min(680, window.innerHeight - 96) };
}

function clampGeom(g: { x: number; y: number; w: number; h: number }) {
  const w = Math.min(Math.max(340, g.w), window.innerWidth - 16);
  const h = Math.min(Math.max(360, g.h), window.innerHeight - 56);
  return { w, h, x: Math.min(Math.max(8, g.x), window.innerWidth - w - 8), y: Math.min(Math.max(44, g.y), window.innerHeight - h - 8) };
}

/** Keeps small context windows from overflowing: drops the oldest turns (never starting on a
 *  tool result) and shortens tool results older than the newest step. */
function trim(convo: any[]): any[] {
  let out = convo.slice(-40);
  while (out.length && out[0].role !== 'user') out = out.slice(1);
  let lastStep = out.length;
  while (lastStep > 0 && out[lastStep - 1].role === 'tool') lastStep--;
  return out.map((m, i) =>
    m.role === 'tool' && i < lastStep && m.content.length > 1200 ? { ...m, content: `${m.content.slice(0, 1200)}…(older result shortened – call the tool again if needed)` } : m,
  );
}

export function AiWindow() {
  const open = useApp((s) => s.aiOpen);
  const setOpen = useApp((s) => s.setAiOpen);
  const ask = useApp((s) => s.aiAsk);
  const settings = useApp((s) => s.settings);
  const saveSettings = useApp((s) => s.saveSettings);
  const ai = settings?.ai;

  const [items, setItems] = useState<Item[]>([]);
  const [input, setInput] = useState('');
  const [busy, setBusy] = useState(false);
  const [approval, setApproval] = useState<Approval | null>(null);
  const [showSettings, setShowSettings] = useState(false);
  const [geom, setGeom] = useState(loadGeom);
  const [max, setMax] = useState(false);
  const convo = useRef<any[]>([]);
  const streamId = useRef<number | null>(null);
  const stopped = useRef(false);
  const allowAll = useRef(false);
  const noTools = useRef(false);
  const bottom = useRef<HTMLDivElement>(null);
  const lastAsk = useRef(0);

  useEffect(() => {
    bottom.current?.scrollIntoView({ block: 'end' });
  }, [items, approval]);
  useEffect(() => {
    try {
      localStorage.setItem(GEOM_KEY, JSON.stringify(geom));
    } catch {
      /* not important */
    }
  }, [geom]);
  useEffect(() => {
    const h = () => setGeom((g) => clampGeom(g));
    window.addEventListener('resize', h);
    return () => window.removeEventListener('resize', h);
  }, []);
  useEffect(() => {
    if (open && ai && !ai.model && !ai.baseUrl) setShowSettings(true);
  }, [open, ai]);

  const sendRef = useRef<(q: string) => void>(() => {});
  useEffect(() => {
    if (ask && ask.seq !== lastAsk.current) {
      lastAsk.current = ask.seq;
      sendRef.current(ask.text);
    }
  }, [ask]);

  if (!settings || !ai) return null;

  const setAi = (patch: Partial<AiSettings>) => saveSettings({ ...settings, ai: { ...ai, ...patch } });

  const patchLast = (f: (it: Item) => Item) => setItems((list) => (list.length ? [...list.slice(0, -1), f(list[list.length - 1])] : list));

  /** One streamed model turn. Resolves with the tool calls it asked for. */
  function complete(useTools: boolean): Promise<{ calls: AiToolCall[]; text: string } | { error: string }> {
    return new Promise((resolve) => {
      let text = '';
      setItems((l) => [...l, { kind: 'assistant', text: '', thinking: '', streaming: true }]);
      const messages = [{ role: 'system', content: systemPrompt(useTools) }, ...trim(convo.current)];
      api
        .aiChat(ai!, messages, useTools ? toolSpecs() : [], (e: AiEvent) => {
          if (e.kind === 'delta') {
            text += e.text;
            patchLast((it) => (it.kind === 'assistant' ? { ...it, text: it.text + e.text } : it));
          } else if (e.kind === 'thinking') {
            patchLast((it) => (it.kind === 'assistant' ? { ...it, thinking: it.thinking + e.text } : it));
          } else {
            streamId.current = null;
            // Drop the bubble if the turn was only tool calls.
            setItems((l) => {
              const last = l[l.length - 1];
              if (last?.kind !== 'assistant') return l;
              if (!last.text.trim() && !last.thinking.trim()) return l.slice(0, -1);
              return [...l.slice(0, -1), { ...last, streaming: false }];
            });
            resolve(e.kind === 'done' ? { calls: e.toolCalls, text } : { error: e.text });
          }
        })
        .then((id) => (streamId.current = id))
        .catch((err) => resolve({ error: errMsg(err) }));
    });
  }

  async function approve(label: string): Promise<'yes' | 'all' | 'no'> {
    if (ai!.approval === 'auto' || allowAll.current) return 'yes';
    return new Promise((resolve) => setApproval({ label, resolve }));
  }

  async function runCall(call: AiToolCall): Promise<string> {
    const tool = TOOL_MAP.get(call.name);
    if (!tool) return `Unknown tool "${call.name}". Available: ${[...TOOL_MAP.keys()].join(', ')}.`;
    let args: Record<string, any>;
    let label: string;
    try {
      args = parseArgs(call.arguments);
      label = tool.label(args);
    } catch (e) {
      setItems((l) => [...l, { kind: 'tool', name: call.name, label: call.name, status: 'error', result: errMsg(e) }]);
      return `Error: ${errMsg(e)}`;
    }
    setItems((l) => [...l, { kind: 'tool', name: call.name, label, status: tool.change ? 'waiting' : 'running' }]);
    if (tool.change) {
      const ok = await approve(label);
      setApproval(null);
      if (ok === 'all') allowAll.current = true;
      if (ok === 'no' || stopped.current) {
        patchLast((it) => (it.kind === 'tool' ? { ...it, status: 'declined' } : it));
        return 'The user declined this change. Do not retry it; ask what they would prefer instead.';
      }
      patchLast((it) => (it.kind === 'tool' ? { ...it, status: 'running' } : it));
    }
    try {
      const out = resultText(await tool.run(args));
      patchLast((it) => (it.kind === 'tool' ? { ...it, status: 'ok', result: out } : it));
      return out;
    } catch (e) {
      const msg = errMsg(e);
      patchLast((it) => (it.kind === 'tool' ? { ...it, status: 'error', result: msg } : it));
      return `Error: ${msg}`;
    }
  }

  async function send(question: string) {
    question = question.trim();
    if (!question || busy) return;
    setInput('');
    setBusy(true);
    stopped.current = false;
    convo.current.push({ role: 'user', content: question });
    setItems((l) => [...l, { kind: 'user', text: question }]);
    let changed = false;
    let nudged = false;
    try {
      for (let round = 0; round < MAX_ROUNDS && !stopped.current; round++) {
        const useTools = ai!.tools && !noTools.current;
        const r = await complete(useTools);
        if ('error' in r) {
          // Models without tool support: carry on as a plain chat.
          if (useTools && /tool|function/i.test(r.error) && /support|not|invalid|unknown/i.test(r.error)) {
            noTools.current = true;
            setItems((l) => [...l, { kind: 'notice', tone: 'info', text: "This model doesn't support tools, so I can only give advice in this chat. Pick a tool-capable model (Qwen 3, Llama 3.1+, Mistral, gpt-oss) to let me act." }]);
            round--;
            continue;
          }
          setItems((l) => [...l, { kind: 'notice', tone: 'error', text: r.error }]);
          break;
        }
        const calls = r.calls;
        convo.current.push({
          role: 'assistant',
          content: r.text,
          ...(calls.length ? { tool_calls: calls.map((c) => ({ id: c.id, type: 'function', function: { name: c.name, arguments: c.arguments || '{}' } })) } : {}),
        });
        if (!calls.length) {
          // Small models sometimes say "I've updated…" without calling a tool. Check once.
          if (!changed && !nudged && CLAIM.test(r.text)) {
            nudged = true;
            setItems((l) => [...l, { kind: 'notice', tone: 'info', text: 'No change was actually made yet – asking the assistant to do it…' }]);
            convo.current.push({ role: 'user', content: '(Message from the app, not the user:) No change was made – you did not call a tool. Call the right tool now to do it, or say clearly that it was not done.' });
            continue;
          }
          break;
        }
        for (const c of calls) {
          const out = stopped.current ? 'Stopped by the user.' : await runCall(c);
          if (TOOL_MAP.get(c.name)?.change && !out.startsWith('Error') && !out.startsWith('The user declined')) changed = true;
          convo.current.push({ role: 'tool', tool_call_id: c.id, name: c.name, content: out });
        }
        if (round === MAX_ROUNDS - 1) setItems((l) => [...l, { kind: 'notice', tone: 'info', text: 'Paused after many steps – say “continue” to keep going.' }]);
      }
    } catch (e) {
      setItems((l) => [...l, { kind: 'notice', tone: 'error', text: `Something went wrong: ${errMsg(e)}` }]);
    } finally {
      setBusy(false);
      setApproval(null);
    }
  }
  sendRef.current = send;

  function stop() {
    stopped.current = true;
    if (streamId.current != null) api.aiStop(streamId.current);
    approval?.resolve('no');
  }

  function newChat() {
    convo.current = [];
    allowAll.current = false;
    noTools.current = false;
    setItems([]);
  }

  function drag(e: RPointerEvent, mode: 'move' | 'resize') {
    if (max || (e.target as HTMLElement).closest('button') && mode === 'move') return;
    e.preventDefault();
    const start = { px: e.clientX, py: e.clientY, ...geom };
    const move = (ev: PointerEvent) => {
      const dx = ev.clientX - start.px;
      const dy = ev.clientY - start.py;
      setGeom(clampGeom(mode === 'move' ? { ...start, x: start.x + dx, y: start.y + dy } : { ...start, w: start.w + dx, h: start.h + dy }));
    };
    const up = () => {
      window.removeEventListener('pointermove', move);
      window.removeEventListener('pointerup', up);
    };
    window.addEventListener('pointermove', move);
    window.addEventListener('pointerup', up);
  }

  const box = max ? { left: 24, top: 52, width: window.innerWidth - 48, height: window.innerHeight - 76 } : { left: geom.x, top: geom.y, width: geom.w, height: geom.h };

  return (
    <AnimatePresence>
      {open && (
        <motion.div
          className="fixed z-40 flex flex-col overflow-hidden rounded-xl border border-line-strong bg-panel shadow-pop"
          style={box}
          initial={{ opacity: 0, scale: 0.96, y: 8 }}
          animate={{ opacity: 1, scale: 1, y: 0 }}
          exit={{ opacity: 0, scale: 0.96, y: 8 }}
          transition={{ duration: 0.14, ease: 'easeOut' }}
        >
          <div onPointerDown={(e) => drag(e, 'move')} onDoubleClick={() => setMax(!max)} className="flex h-10 shrink-0 cursor-grab items-center gap-2 border-b border-line bg-bg-raised px-3 select-none active:cursor-grabbing">
            <Bot className="size-4 text-accent" />
            <span className="text-[12.5px] font-semibold text-fg">AI Assistant</span>
            <span className="truncate text-[11px] text-fg-4">{ai.model || 'local model'} · on your PC</span>
            <span className="flex-1" />
            <IconButton label="New chat" disabled={busy || !items.length} onClick={newChat}>
              <Plus />
            </IconButton>
            <IconButton label="Assistant settings" active={showSettings} onClick={() => setShowSettings(!showSettings)}>
              <Settings2 />
            </IconButton>
            <IconButton label={max ? 'Restore' : 'Maximize'} onClick={() => setMax(!max)}>
              {max ? <Minimize2 /> : <Maximize2 />}
            </IconButton>
            <IconButton label="Close (Ctrl+J)" onClick={() => setOpen(false)}>
              <X />
            </IconButton>
          </div>

          {showSettings && <SettingsPanel ai={ai} setAi={setAi} />}

          <div className="min-h-0 flex-1 space-y-2.5 overflow-y-auto px-3 py-3 text-[12.5px]">
            {items.length === 0 && (
              <div className="space-y-2 px-1 py-2 text-fg-3">
                <p className="text-[13px] text-fg-2">Hi! I can look at your servers' consoles, crash reports, mods and config files, and change settings, files, automation and more – always asking you before I change anything.</p>
                <p className="text-[11.5px]">Runs on your own PC through Ollama, LM Studio or any OpenAI-compatible server. Nothing goes to the internet.</p>
              </div>
            )}
            {items.map((it, i) => (
              <ItemView key={i} it={it} />
            ))}
            {approval && (
              <div className="rounded-lg border border-warn/40 bg-warn/8 p-2.5">
                <div className="mb-2 flex items-start gap-2 text-[12px] text-fg">
                  <ShieldAlert className="mt-0.5 size-4 shrink-0 text-warn" />
                  <span>
                    <b>Allow this change?</b>
                    <br />
                    <span className="text-fg-2">{approval.label}</span>
                  </span>
                </div>
                <div className="flex flex-wrap gap-1.5">
                  <Button size="sm" variant="primary" icon={<Check className="size-3.5" />} onClick={() => approval.resolve('yes')}>
                    Allow
                  </Button>
                  <Button size="sm" variant="secondary" onClick={() => approval.resolve('all')}>
                    Allow all in this chat
                  </Button>
                  <Button size="sm" variant="ghost" onClick={() => approval.resolve('no')}>
                    Decline
                  </Button>
                </div>
              </div>
            )}
            <div ref={bottom} />
          </div>

          {!busy && items.length === 0 && (
            <div className="flex flex-wrap gap-1.5 px-3 pb-2">
              {ASKS.map(([label, q]) => (
                <button key={label} onClick={() => send(q)} className="rounded-full border border-line-strong px-2.5 py-1 text-[11.5px] text-fg-2 transition-colors hover:border-accent/50 hover:text-fg">
                  {label}
                </button>
              ))}
            </div>
          )}
          <div className="flex shrink-0 items-end gap-2 border-t border-line p-2.5">
            <textarea
              value={input}
              rows={2}
              placeholder="Ask anything… (Enter sends, Shift+Enter = new line)"
              onChange={(e) => setInput(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === 'Enter' && !e.shiftKey) {
                  e.preventDefault();
                  send(input);
                }
              }}
              className="min-h-[40px] flex-1 resize-none rounded-lg border border-line-strong bg-bg-raised px-2.5 py-2 text-[12.5px] text-fg outline-none placeholder:text-fg-4 focus:border-accent/60"
            />
            {busy ? (
              <Button variant="danger" icon={<Square className="size-3.5" />} onClick={stop}>
                Stop
              </Button>
            ) : (
              <Button variant="primary" icon={<Send className="size-3.5" />} disabled={!input.trim()} onClick={() => send(input)}>
                Send
              </Button>
            )}
          </div>
          {!max && <div onPointerDown={(e) => drag(e, 'resize')} className="absolute right-0 bottom-0 size-3.5 cursor-nwse-resize" title="Resize" />}
        </motion.div>
      )}
    </AnimatePresence>
  );
}

function ItemView({ it }: { it: Item }) {
  const [openResult, setOpenResult] = useState(false);
  if (it.kind === 'user') return <div className="ml-8 rounded-lg bg-accent-dim px-3 py-2 whitespace-pre-wrap text-fg">{it.text}</div>;
  if (it.kind === 'notice') return <div className={cn('rounded-lg border px-3 py-2 text-[12px]', it.tone === 'error' ? 'border-err/30 bg-err/8 text-err' : 'border-info/30 bg-info/8 text-fg-2')}>{it.text}</div>;
  if (it.kind === 'tool') {
    const icon =
      it.status === 'running' || it.status === 'waiting' ? <Loader2 className="size-3.5 animate-spin text-fg-3" /> : it.status === 'ok' ? <Check className="size-3.5 text-ok" /> : it.status === 'declined' ? <X className="size-3.5 text-fg-4" /> : <X className="size-3.5 text-err" />;
    return (
      <div className="rounded-md border border-line bg-bg-raised/60 text-[11.5px]">
        <button className="flex w-full items-center gap-2 px-2 py-1.5 text-left text-fg-3 hover:text-fg-2" onClick={() => setOpenResult(!openResult)} disabled={!it.result}>
          <Wrench className="size-3.5 shrink-0" />
          <span className="min-w-0 flex-1 truncate">
            {it.label}
            {it.status === 'waiting' && ' – waiting for you'}
            {it.status === 'declined' && ' – declined'}
          </span>
          {icon}
          {it.result && (openResult ? <ChevronDown className="size-3.5" /> : <ChevronRight className="size-3.5" />)}
        </button>
        {openResult && it.result && <pre className="max-h-56 overflow-auto border-t border-line px-2 py-1.5 font-mono text-[10.5px] whitespace-pre-wrap text-fg-3">{it.result}</pre>}
      </div>
    );
  }
  return (
    <div className="mr-4 text-fg">
      {it.thinking && (
        <details className="mb-1 text-[11.5px] text-fg-4" open={it.streaming && !it.text}>
          <summary className="cursor-pointer select-none">{it.streaming && !it.text ? 'Thinking…' : 'Thought process'}</summary>
          <div className="mt-1 border-l border-line pl-2 whitespace-pre-wrap">{it.thinking}</div>
        </details>
      )}
      {it.text ? <Markdown text={it.text} /> : it.streaming && !it.thinking && <Loader2 className="size-4 animate-spin text-fg-4" />}
    </div>
  );
}

function SettingsPanel({ ai, setAi }: { ai: AiSettings; setAi: (p: Partial<AiSettings>) => void }) {
  const [url, setUrl] = useState(ai.baseUrl);
  const [models, setModels] = useState<string[]>([]);
  const [status, setStatus] = useState<{ ok: boolean; text: string } | null>(null);
  const [testing, setTesting] = useState(false);
  useEffect(() => {
    setUrl(ai.baseUrl);
  }, [ai.baseUrl]);

  async function test(base = ai.baseUrl) {
    setTesting(true);
    setStatus(null);
    try {
      const list = await api.aiModels({ ...ai, baseUrl: base });
      setModels(list);
      setStatus({ ok: true, text: list.length ? `Connected – ${list.length} model${list.length === 1 ? '' : 's'}.` : 'Connected, but no model is loaded.' });
    } catch (e) {
      setStatus({ ok: false, text: errMsg(e) });
    } finally {
      setTesting(false);
    }
  }

  return (
    <div className="max-h-[60%] shrink-0 space-y-2.5 overflow-y-auto border-b border-line bg-bg-raised/50 px-3 py-3 text-[12px]">
      <div className="flex flex-wrap gap-1.5">
        {PRESETS.map(([name, u, hint]) => (
          <button
            key={name}
            title={hint}
            onClick={() => {
              setAi({ baseUrl: u });
              test(u);
            }}
            className={cn('rounded-md border px-2 py-1 text-[11.5px]', ai.baseUrl === u ? 'border-accent/60 bg-accent-dim text-fg' : 'border-line-strong text-fg-2 hover:text-fg')}
          >
            {name}
          </button>
        ))}
      </div>
      <label className="block space-y-1">
        <span className="text-fg-3">Server address (OpenAI-compatible, ends in /v1)</span>
        <Input value={url} mono spellCheck={false} onChange={(e) => setUrl(e.target.value)} onBlur={() => url !== ai.baseUrl && setAi({ baseUrl: url.trim() })} />
      </label>
      <label className="block space-y-1">
        <span className="text-fg-3">Model</span>
        <div className="flex gap-1.5">
          <Input list="ai-models" value={ai.model} mono spellCheck={false} placeholder="(first model the server lists)" onChange={(e) => setAi({ model: e.target.value })} />
          <datalist id="ai-models">
            {models.map((m) => (
              <option key={m} value={m} />
            ))}
          </datalist>
          <Button size="md" loading={testing} onClick={() => test()}>
            Test
          </Button>
        </div>
      </label>
      {status && <div className={status.ok ? 'text-ok' : 'text-err'}>{status.text}</div>}
      <label className="block space-y-1">
        <span className="text-fg-3">API key (only if your server needs one)</span>
        <SecretInput value={ai.apiKey} onChange={(v) => setAi({ apiKey: v })} placeholder="none" />
      </label>
      <div className="flex items-center justify-between gap-2">
        <span className="text-fg-2">Changes</span>
        <Segmented
          size="sm"
          value={ai.approval === 'auto' ? 'auto' : 'ask'}
          onChange={(v) => setAi({ approval: v })}
          options={[
            { value: 'ask', label: 'Ask me first' },
            { value: 'auto', label: 'Allow automatically' },
          ]}
        />
      </div>
      <div className="flex items-center justify-between gap-2">
        <span className="text-fg-2">Let the assistant use the app (tools)</span>
        <Toggle size="sm" checked={ai.tools} onChange={(v) => setAi({ tools: v })} />
      </div>
      <div className="flex items-center justify-between gap-2">
        <span className="text-fg-2">Creativity ({ai.temperature.toFixed(1)})</span>
        <input type="range" min={0} max={1} step={0.1} value={ai.temperature} onChange={(e) => setAi({ temperature: +e.target.value })} className="w-32 accent-[var(--color-accent)]" />
      </div>
      <p className="text-[11px] leading-relaxed text-fg-4">
        Best with a tool-capable model: <code>qwen3:8b</code> (8 GB+ VRAM), <code>qwen3:14b</code> or <code>gpt-oss:20b</code> (16 GB+), <code>llama3.1:8b</code>. Reading logs and files is always allowed; changes follow the setting above.
      </p>
      <p className="text-[11px] leading-relaxed text-fg-4">
        Give the model a context of <b>16k tokens or more</b> if it forgets your question mid-task (Ollama: Settings → Context length; LM Studio: the model's load settings).
      </p>
    </div>
  );
}
