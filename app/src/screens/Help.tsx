import { useEffect, useRef, useState } from 'react';
import { api, type AppInfo, type SelfTestResult } from '../api';
import { Check } from '../components';
import { useApp, useT } from '../context';
import { fill } from '../i18n';
import { useCableBroken } from '../useCableBroken';

const routeNames: Record<string, { da: string; en: string }> = {
  micToDiscord: { da: 'Dig → Discord', en: 'You → Discord' },
  claudeToDiscord: { da: 'Claude → Discord', en: 'Claude → Discord' },
  micToClaude: { da: 'Dig → Claude', en: 'You → Claude' },
  discordToClaude: { da: 'Discord → Claude', en: 'Discord → Claude' },
};

export function Help({ info, onHome }: { info: AppInfo; onHome: () => void }) {
  const t = useT();
  const { status, lang, toast, config } = useApp();
  // Anything the overview shows as a problem: then the sound is not "fine".
  const cableBroken = useCableBroken(status, info);
  const problem = !!status && (
    !info.supported
    || (!status.cableOk && !info.cableInstalled) || cableBroken
    || status.micState === 'blocked' || status.micState === 'missing'
    || status.discordNeedsRestart
    || (status.discordUsesCable === 'no' && config.routeDiscord));
  const [running, setRunning] = useState(false);
  const [results, setResults] = useState<SelfTestResult[] | null>(null);
  // Plain verdict: dropouts in the last ~10 seconds, or a long delay.
  const history = useRef<{ t: number; x: number }[]>([]);
  const [choppy, setChoppy] = useState(false);
  useEffect(() => {
    if (!status) return;
    const now = performance.now();
    const h = history.current.filter((e) => now - e.t < 10_000);
    h.push({ t: now, x: status.xruns });
    history.current = h;
    setChoppy(h[h.length - 1].x - h[0].x > 4800 || status.latencyMs > 80);
  }, [status]);

  return (
    <div className="page">
      <header><h1>{t.help.title}</h1></header>

      <div className="panel">
        <div className="row">
          <div className="text"><span className="label">{t.help.selfTestTitle}</span><span className="small">{t.help.selfTestHint}</span></div>
          <button className="btn primary" disabled={running} onClick={async () => {
            setRunning(true);
            try { setResults(await api.selfTest()); }
            catch (e) {
              const key = String(e).replace(/^error\./, '') as keyof typeof t.errors;
              toast((t.errors[key] as string | undefined) ?? t.errors.generic);
            } finally { setRunning(false); }
          }}>{running ? t.help.running : t.help.selfTest}</button>
        </div>
        {results && (
          <ul className="results">
            {results.map((r) => (
              <li key={r.route}><Check state={r.passed ? 'ok' : 'warn'}>{routeNames[r.route]?.[lang] ?? r.route}: {r.passed ? t.help.passed : t.help.failed}</Check></li>
            ))}
          </ul>
        )}
        {results?.some((r) => !r.passed) && <p className="muted" style={{ padding: '0 1.25rem 1rem', margin: 0 }}>{t.help.failedNext}</p>}
      </div>

      <div className="panel">
        <div className="row">
          <div className="text"><span className="label">{t.help.krispTitle}</span><span className="small">{t.notices.krispBody}</span></div>
        </div>
      </div>

      <h2 className="section-title">{t.help.health}</h2>
      <div className="panel">
        <div className="row">
          {!status
            ? <Check state="wait">{t.help.healthStarting}</Check>
            : problem
              ? <><Check state="warn">{t.help.healthProblem}</Check><button className="btn secondary" onClick={onHome}>{t.help.toHome}</button></>
              : choppy ? <Check state="warn">{t.help.healthChoppy}</Check> : <Check state="ok">{t.help.healthGood}</Check>}
        </div>
        <details className="row details">
          <summary className="small">{t.help.numbers}</summary>
          <div className="stats">
            <div className="stat"><span className="small">{t.help.latency}</span><span className="value">{status ? `${Math.round(status.latencyMs)} ms` : '–'}</span></div>
            <div className="stat"><span className="small">{t.help.drift}</span><span className="value">{status ? `${Math.round(status.driftPpm)} ppm` : '–'}</span></div>
            <div className="stat"><span className="small">{t.help.dropouts}</span><span className="value">{status ? status.xruns : '–'}</span></div>
          </div>
        </details>
      </div>

      <div className="panel">
        <div className="row">
          <div className="text"><span className="label">{t.help.reportTitle}</span><span className="small">{t.help.reportHint}</span></div>
          <button className="btn secondary" onClick={async () => { try { await api.saveReport(); toast(t.help.reportSaved); } catch { toast(t.errors.report); } }}>{t.help.report}</button>
        </div>
        <div className="row">
          <div className="text"><span className="label">{t.help.contact}</span><span className="small">{t.help.contactHint}</span></div>
          <button className="btn secondary" onClick={() => api.openPlace('support')}>{t.help.contact}</button>
        </div>
        <div className="row">
          <div className="text"><span className="label">{t.help.logsTitle}</span></div>
          <button className="btn secondary" onClick={() => api.openPlace('logs')}>{t.help.logs}</button>
        </div>
      </div>

      <h2 className="section-title">{t.help.about}</h2>
      <div className="panel">
        <div className="row">
          <div className="text">
            <span className="label">{fill(t.help.version, { v: info.version })}</span>
            <span className="small">{t.help.vbAudio}</span>
          </div>
          <button className="btn quiet" onClick={() => api.openPlace('vbAudioDonate')}>{t.help.vbAudioDonate}</button>
        </div>
      </div>
    </div>
  );
}
