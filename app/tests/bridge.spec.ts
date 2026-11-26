// Verifies bridge.js in a real Chromium against the CI voice page served
// with a strict nonce CSP. A simulated WebView2 host feeds a 1000 + 2500 Hz
// mix through host messages, exactly as the Rust bridge does.
import { expect, test } from '@playwright/test';
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';

const root = fileURLToPath(new URL('../..', import.meta.url));
const bridge = readFileSync(`${root}/app/src-tauri/src/claude/bridge.js`, 'utf8');
const page_ = readFileSync(`${root}/tools/audiotest/testpage/test.html`, 'utf8');

const fakeHost = `
(() => {
  const listeners = [];
  const sent = [];
  window.__hostSent = sent;
  window.chrome = window.chrome || {};
  window.chrome.webview = {
    addEventListener: (t, f) => { if (t === 'message') listeners.push(f); },
    removeEventListener: () => {},
    postMessage: (m) => sent.push(m),
  };
  // host clock: 10 ms blocks of 1000 Hz + 2500 Hz, f32 mono, base64
  let phase1 = 0, phase2 = 0;
  setInterval(() => {
    const n = 480, f = new Float32Array(n);
    for (let i = 0; i < n; i++) { f[i] = 0.25 * Math.sin(phase1) + 0.25 * Math.sin(phase2); phase1 += 2 * Math.PI * 1000 / 48000; phase2 += 2 * Math.PI * 2500 / 48000; }
    const b = new Uint8Array(f.buffer); let s = ''; for (let i = 0; i < b.length; i++) s += String.fromCharCode(b[i]);
    const ev = { data: 'ct:a:' + btoa(s) };
    listeners.forEach((l) => l(ev));
  }, 10);
})();
`;

test('bridge feeds getUserMedia under a strict CSP', async ({ page }) => {
  test.setTimeout(60_000);
  const nonce = 'testnonce123';
  let report: any = null;
  await page.route('https://claude-ci.test/test.html', (r) =>
    r.fulfill({
      status: 200,
      headers: {
        'content-type': 'text/html',
        'content-security-policy': `default-src 'none'; script-src 'nonce-${nonce}' 'unsafe-eval'; script-src-attr 'none'; style-src 'unsafe-inline'; connect-src 'self'; media-src blob:; worker-src blob:`,
      },
      body: page_.replace('__NONCE__', nonce),
    }),
  );
  await page.route('https://claude-ci.test/report', async (r) => { report = JSON.parse(r.request().postData() ?? '{}'); await r.fulfill({ status: 204 }); });
  await page.addInitScript(fakeHost);
  await page.addInitScript(bridge);
  await page.goto('https://claude-ci.test/test.html');
  page.on('console', (m) => console.log('page:', m.text()));
  page.on('pageerror', (e) => console.log('pageerror:', e.message));
  await expect.poll(() => report, { timeout: 30_000 }).not.toBeNull().catch(async (e) => {
    console.log('page log:', await page.locator('#log').textContent());
    throw e;
  });
  const sent: string[] = await page.evaluate(() => (window as any).__hostSent);
  console.log(JSON.stringify(report, null, 1));
  console.log('to host:', sent.map((m) => JSON.parse(m).type));
  expect(report.errors).toEqual([]);
  const step = (n: string) => report.steps.find((s: any) => s.name === n);
  expect(step('permission').state).toBe('granted');
  expect(step('devices').audioinput[0]).toContain('Claude Talk');
  expect(step('track').label).toBe('Claude Talk');
  expect(step('track').settings.echoCancellation).toBe(false);
  expect(report.levels['1000']).toBeGreaterThan(-20);
  expect(report.levels['2500']).toBeGreaterThan(-20);
  expect(report.levels['3000']).toBeLessThan(report.levels['1000'] - 60);
  expect(step('mediarecorder').bytes).toBeGreaterThan(0);
  expect(step('webrtc').remoteState).toBe('live');
  expect(sent.some((m) => JSON.parse(m).type === 'mic' && JSON.parse(m).active === true)).toBe(true);
  expect(report.webrtcEnergy).toBeGreaterThan(0);
  // the page stopped its track, but kept a clone for the latency detector:
  // the microphone must stay in use while any clone lives
  const mics = sent.map((m) => JSON.parse(m)).filter((m) => m.type === 'mic');
  expect(mics[mics.length - 1].active).toBe(true);
});

test('a clone keeps the microphone alive; stopping every track releases it', async ({ page }) => {
  await page.route('https://claude-ci.test/blank.html', (r) => r.fulfill({ status: 200, headers: { 'content-type': 'text/html' }, body: '<!doctype html><title>x</title>' }));
  await page.addInitScript(fakeHost);
  await page.addInitScript(bridge);
  await page.goto('https://claude-ci.test/blank.html');
  const result = await page.evaluate(async () => {
    const mic = () => (window as any).__hostSent.map((m: string) => JSON.parse(m)).filter((m: any) => m.type === 'mic').pop()?.active;
    const s = await navigator.mediaDevices.getUserMedia({ audio: true });
    const t = s.getAudioTracks()[0];
    const c = t.clone();
    const c2 = c.clone();
    t.stop();
    const afterOriginal = mic();
    c.stop();
    const afterFirstClone = mic();
    c.stop(); // twice is harmless
    c2.stop();
    return { cloneLabel: c2.label, cloneState: c.readyState, afterOriginal, afterFirstClone, afterAll: mic() };
  });
  expect(result.cloneLabel).toBe('Claude Talk');
  expect(result.afterOriginal).toBe(true);
  expect(result.afterFirstClone).toBe(true);
  expect(result.afterAll).toBe(false);
});
