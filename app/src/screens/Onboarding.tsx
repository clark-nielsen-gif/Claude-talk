import { useEffect, useState } from 'react';
import { api, type AppInfo } from '../api';
import { Icon, Notice } from '../components';
import { useApp, useT } from '../context';
import { useInstallCable } from '../useInstallCable';

type Step = 'welcome' | 'setup' | 'ready';

function Dots({ at }: { at: number }) {
  return <div className="dots" aria-hidden="true">{[0, 1, 2].map((i) => <span key={i} className={i === at ? 'on' : ''} />)}</div>;
}

/** First start: at most three screens, then the overview. */
export function Onboarding({ info, initial = 'welcome', onDone }: { info: AppInfo; initial?: Step; onDone: () => void }) {
  const t = useT();
  const { status, config, toast } = useApp();
  const [step, setStep] = useState<Step>(initial);
  const [shown, setShown] = useState(0); // how many setup steps have been revealed
  const [slow, setSlow] = useState(false); // setup has waited longer than it should
  const { installing, install } = useInstallCable();

  // Each step is ticked off only when the real condition is true.
  const checks = [
    { label: t.onboarding.stepCable, done: info.cableInstalled || !!status?.cableOk },
    { label: t.onboarding.stepMic, done: status?.micState === 'ok' || status?.micState === 'missing' || status?.micState === 'blocked' },
    { label: t.onboarding.stepEngine, done: !!status?.engineRunning },
    { label: t.onboarding.stepDiscord, done: !!status?.engineRunning },
  ];
  const allDone = checks.every((c) => c.done);
  // What can't finish by itself: shown at once when it is certain, otherwise
  // after 12 s – always with a way forward.
  const cableMissing = !info.cableInstalled && !!status?.engineRunning && !status.cableOk;
  const problem = !info.supported ? 'unsupported' : cableMissing && (slow || shown >= checks.length) ? 'cable' : slow && !allDone ? 'slow' : null;

  useEffect(() => {
    if (step !== 'setup' || allDone) return;
    const id = setTimeout(() => setSlow(true), 12_000);
    return () => clearTimeout(id);
  }, [step, allDone]);

  useEffect(() => {
    if (step !== 'setup') return;
    const id = setInterval(() => setShown((n) => Math.min(checks.length, n + 1)), 650);
    return () => clearInterval(id);
  }, [step, checks.length]);

  useEffect(() => {
    if (step === 'setup' && allDone && shown >= checks.length) {
      const id = setTimeout(() => setStep('ready'), 700);
      return () => clearTimeout(id);
    }
  }, [step, allDone, shown, checks.length]);

  const progress = step === 'setup' ? Math.round((100 * checks.filter((c, i) => c.done && i < shown).length) / checks.length) : 0;

  return (
    <main className="onboard">
      <div className="card">
        {step === 'welcome' && (
          <>
            <div className="wave" aria-hidden="true">{[0.36, 0.64, 1, 0.64, 0.36].map((h, i) => <span key={i} style={{ height: `${h * 100}%` }} />)}</div>
            <h1>{t.onboarding.welcomeTitle}</h1>
            <p className="lead">{t.onboarding.welcomeBody}</p>
            <div><button className="btn primary large" onClick={() => setStep('setup')} autoFocus>{t.onboarding.start}</button></div>
            <Dots at={0} />
          </>
        )}
        {step === 'setup' && (
          <>
            <h1>{t.onboarding.setupTitle}</h1>
            <p className="lead">{problem ? t.onboarding.setupProblemBody : t.onboarding.setupBody}</p>
            <div className="progress" role="progressbar" aria-valuenow={progress} aria-valuemin={0} aria-valuemax={100}><div style={{ width: `${progress}%` }} /></div>
            <ul className="steps">
              {checks.map((c, i) => {
                const revealed = i < shown;
                const ok = revealed && c.done;
                return (
                  <li key={c.label} className={ok ? '' : 'pending'}>
                    {ok ? <span className="check ok"><span className="dot">{Icon.check()}</span></span> : <span className="spinner" aria-hidden="true" />}
                    {c.label}
                  </li>
                );
              })}
            </ul>
            {problem === 'unsupported' && <Notice kind="error" title={t.errors.unsupportedTitle} body={t.errors.unsupportedBody} action={t.errors.unsupportedAction} onAction={() => api.openPlace('windowsUpdate')} />}
            {problem === 'cable' && <Notice kind="error" title={t.errors.cableMissingTitle} body={t.onboarding.cableMissingBody} action={installing ? t.errors.cableInstalling : t.errors.cableMissingAction} onAction={installing ? undefined : install} />}
            {problem === 'slow' && <Notice kind="warn" title={t.onboarding.slowTitle} body={t.onboarding.slowBody} />}
            {problem && <div><button className="btn secondary" onClick={onDone}>{t.onboarding.toOverview}</button></div>}
            <Dots at={1} />
          </>
        )}
        {step === 'ready' && (
          <>
            <div className="wave" aria-hidden="true">{[0.36, 0.64, 1, 0.64, 0.36].map((h, i) => <span key={i} style={{ height: `${h * 100}%` }} />)}</div>
            <h1>{t.onboarding.readyTitle}</h1>
            <p className="lead">{t.onboarding.readyBody}</p>
            {config.instructions.autoSend && <p className="muted">{t.onboarding.readyAutoSend}</p>}
            <div style={{ display: 'flex', gap: '0.75rem' }}>
              <button className="btn primary large" autoFocus onClick={() => api.openClaude().then(onDone, () => toast(t.errors.claudeWindow))}>{t.onboarding.openClaude}</button>
              <button className="btn secondary large" onClick={onDone}>{t.onboarding.toOverview}</button>
            </div>
            <Dots at={2} />
          </>
        )}
      </div>
    </main>
  );
}
