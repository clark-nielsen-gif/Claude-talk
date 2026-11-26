import { useEffect, useState } from 'react';
import { api, type AppInfo } from '../api';
import { Icon } from '../components';
import { useApp, useT } from '../context';

type Step = 'welcome' | 'setup' | 'ready';

function Dots({ at }: { at: number }) {
  return <div className="dots" aria-hidden="true">{[0, 1, 2].map((i) => <span key={i} className={i === at ? 'on' : ''} />)}</div>;
}

/** First start: at most three screens, then the overview. */
export function Onboarding({ info, initial = 'welcome', onDone }: { info: AppInfo; initial?: Step; onDone: () => void }) {
  const t = useT();
  const { status } = useApp();
  const [step, setStep] = useState<Step>(initial);
  const [shown, setShown] = useState(0); // how many setup steps have been revealed

  // Each step is ticked off only when the real condition is true.
  const checks = [
    { label: t.onboarding.stepCable, done: info.cableInstalled || !!status?.cableOk },
    { label: t.onboarding.stepMic, done: status?.micState === 'ok' || status?.micState === 'missing' || status?.micState === 'blocked' },
    { label: t.onboarding.stepEngine, done: !!status?.engineRunning },
    { label: t.onboarding.stepDiscord, done: !!status?.engineRunning },
  ];
  const allDone = checks.every((c) => c.done);

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
            <p className="lead">{t.onboarding.setupBody}</p>
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
            <Dots at={1} />
          </>
        )}
        {step === 'ready' && (
          <>
            <div className="wave" aria-hidden="true">{[0.36, 0.64, 1, 0.64, 0.36].map((h, i) => <span key={i} style={{ height: `${h * 100}%` }} />)}</div>
            <h1>{t.onboarding.readyTitle}</h1>
            <p className="lead">{t.onboarding.readyBody}</p>
            <div style={{ display: 'flex', gap: '0.75rem' }}>
              <button className="btn primary large" autoFocus onClick={async () => { await api.openClaude(); onDone(); }}>{t.onboarding.openClaude}</button>
              <button className="btn secondary large" onClick={onDone}>{t.onboarding.toOverview}</button>
            </div>
            <Dots at={2} />
          </>
        )}
      </div>
    </main>
  );
}
