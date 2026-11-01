// Regenerates docs/screenshots/*.png from the browser preview (mock backend).
// Usage: npm run dev   (in another terminal)   then   node scripts/screenshots.mjs
import { spawn } from 'node:child_process';
import { mkdirSync, mkdtempSync, writeFileSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';

const root = resolve(import.meta.dirname, '..');
const out = join(root, 'docs', 'screenshots');
mkdirSync(out, { recursive: true });
const URL = 'http://localhost:1430/';
const PORT = 9555;
const edge = ['C:/Program Files (x86)/Microsoft/Edge/Application/msedge.exe', 'C:/Program Files/Microsoft/Edge/Application/msedge.exe'].find(existsSync);
if (!edge) throw new Error('Microsoft Edge not found');

const profile = mkdtempSync(join(tmpdir(), 'mcsm-shots-'));
const browser = spawn(edge, ['--headless=new', `--remote-debugging-port=${PORT}`, `--user-data-dir=${profile}`, '--window-size=1440,900', '--hide-scrollbars', '--force-device-scale-factor=1', 'about:blank'], { stdio: 'ignore' });
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

let targets = [];
for (let i = 0; i < 40 && !targets.length; i++) {
  await sleep(250);
  try {
    targets = (await (await fetch(`http://127.0.0.1:${PORT}/json`)).json()).filter((t) => t.type === 'page');
  } catch {}
}
const ws = new WebSocket(targets[0].webSocketDebuggerUrl);
await new Promise((r) => (ws.onopen = r));
let seq = 0;
const pending = new Map();
ws.onmessage = (m) => {
  const d = JSON.parse(m.data);
  pending.get(d.id)?.(d);
};
const send = (method, params = {}) => new Promise((r) => { const id = ++seq; pending.set(id, r); ws.send(JSON.stringify({ id, method, params })); });
const evaluate = (expression) => send('Runtime.evaluate', { expression, awaitPromise: true });

await send('Emulation.setDeviceMetricsOverride', { width: 1440, height: 900, deviceScaleFactor: 1, mobile: false });
await send('Page.navigate', { url: URL });
await sleep(2500);

const SERVER = 'a1b2c3d4-0001';
const shots = [
  { name: 'dashboard', active: 'home' },
  { name: 'overview', view: 'overview' },
  { name: 'performance', view: 'performance', wait: 9000 },
  { name: 'console', view: 'console', wait: 5000 },
  { name: 'players', view: 'players' },
  { name: 'settings', view: 'settings' },
  { name: 'software', view: 'software' },
  { name: 'java', view: 'java', server: 'a1b2c3d4-0002' },
  { name: 'port-forwarding', view: 'ports' },
  { name: 'mods', view: 'mods', server: 'a1b2c3d4-0002' },
  { name: 'worlds', view: 'worlds' },
  { name: 'automation', view: 'automation' },
  { name: 'health', view: 'health' },
  { name: 'essentials', view: 'mods', click: 'Essentials' },
  { name: 'player-stats', view: 'players', click: 'Stats' },
  { name: 'assistant', view: 'performance', wait: 6000, ask: 'The server lags when people explore. Find out why and fix it.' },
];
for (const s of shots) {
  const sid = s.server ?? SERVER;
  const ui = s.active === 'home' ? { tabs: [SERVER], active: 'home', views: {} } : { tabs: [SERVER, 'a1b2c3d4-0002'], active: sid, views: { [sid]: s.view } };
  // The AI window reads its position on load.
  const aiGeom = `localStorage.setItem('mcsm.ai.window', JSON.stringify({ x: innerWidth - 500, y: 56, w: 480, h: innerHeight - 76 }));`;
  await evaluate(`localStorage.setItem('mcsm.ui', ${JSON.stringify(JSON.stringify(ui))}); ${aiGeom} location.reload();`);
  await sleep(s.wait ?? 3500);
  if (s.click) {
    // Inner tabs (Segmented buttons) live in component state – click them like a user would.
    await evaluate(`[...document.querySelectorAll('button')].find((b) => b.textContent.trim().startsWith(${JSON.stringify(s.click)}))?.click()`);
    await sleep(s.clickWait ?? 4000);
  }
  if (s.ask) {
    // The AI window (mock model): open it, place it, send a question and wait for the approval card.
    await evaluate(`window.dispatchEvent(new KeyboardEvent('keydown', { key: 'j', ctrlKey: true }))`);
    await sleep(800);
    await evaluate(`const ta = document.querySelector('textarea[placeholder^="Ask anything"]');
      Object.getOwnPropertyDescriptor(HTMLTextAreaElement.prototype, 'value').set.call(ta, ${JSON.stringify(s.ask)});
      ta.dispatchEvent(new Event('input', { bubbles: true }));
      setTimeout(() => [...document.querySelectorAll('button')].find((b) => b.textContent.trim() === 'Send').click(), 100)`);
    await sleep(9000);
  }
  const r = await send('Page.captureScreenshot', { format: 'png' });
  writeFileSync(join(out, `${s.name}.png`), Buffer.from(r.result.data, 'base64'));
  console.log('✓', s.name);
}
ws.close();
browser.kill();
