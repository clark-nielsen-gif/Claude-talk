import { useEffect, useRef, useState, type ReactNode } from 'react';
import { api, type Level } from './api';
import { useT } from './context';

// ---- icons (original, 20px grid, currentColor) ---------------------------------
const I = (d: ReactNode, size = 18) => (
  <svg width={size} height={size} viewBox="0 0 20 20" fill="none" stroke="currentColor" strokeWidth="1.7" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">{d}</svg>
);
export const Icon = {
  mic: () => I(<><rect x="7" y="2.5" width="6" height="10" rx="3" /><path d="M4.5 9.5a5.5 5.5 0 0 0 11 0M10 15v2.5" /></>),
  micOff: () => I(<><path d="M13 9V5.5a3 3 0 0 0-5.6-1.5M7 7.5v2a3 3 0 0 0 4.7 2.5M4.5 9.5a5.5 5.5 0 0 0 8.6 4.5M15.5 9.5c0 .6-.1 1.2-.3 1.7M10 15v2.5M3 3l14 14" /></>),
  speaker: () => I(<><path d="M3.5 7.5h3l4-3.5v12l-4-3.5h-3z" /><path d="M13.5 7a4 4 0 0 1 0 6M15.5 5a7 7 0 0 1 0 10" /></>),
  speakerOff: () => I(<><path d="M3.5 7.5h3l4-3.5v12l-4-3.5h-3z" /><path d="M13.5 8l4 4M17.5 8l-4 4" /></>),
  check: () => I(<path d="M5 10.5l3.2 3L15 6.5" />, 14),
  alert: () => I(<><path d="M10 6.5v4.5" /><circle cx="10" cy="14" r=".6" fill="currentColor" /></>, 14),
  minimize: () => I(<path d="M5 10h10" />, 16),
  maximize: () => I(<rect x="5" y="5" width="10" height="10" rx="1.5" />, 14),
  close: () => I(<path d="M5.5 5.5l9 9M14.5 5.5l-9 9" />, 16),
};

export function Mark({ size = 18 }: { size?: number }) {
  // the app icon, drawn small
  const bars = [0.2, 0.4, 0.58, 0.4, 0.2];
  return (
    <svg className="mark" width={size} height={size} viewBox="0 0 100 100" aria-hidden="true">
      <rect x="4" y="4" width="92" height="92" rx="23" fill="var(--clay)" />
      {bars.map((h, i) => (
        <rect key={i} x={23.5 + i * 11.6} y={50 - h * 50} width="7.4" height={h * 100} rx="3.7" fill="#faf9f5" />
      ))}
    </svg>
  );
}

export function TitleBar() {
  const t = useT();
  return (
    <div className="titlebar" data-tauri-drag-region>
      <Mark />
      <span className="name" data-tauri-drag-region>{t.appName}</span>
      <span className="spacer" data-tauri-drag-region />
      <button className="winbtn" aria-label={t.window.minimize} title={t.window.minimize} onClick={() => api.windowAction('minimize')}>{Icon.minimize()}</button>
      <button className="winbtn" aria-label={t.window.maximize} title={t.window.maximize} onClick={() => api.windowAction('toggleMaximize')}>{Icon.maximize()}</button>
      <button className="winbtn close" aria-label={t.window.close} title={t.window.close} onClick={() => api.windowAction('close')}>{Icon.close()}</button>
    </div>
  );
}

/** Live level as the icon's sound-wave bars. Decays smoothly between updates. */
export function Meter({ level, off = false, label }: { level: Level; off?: boolean; label: string }) {
  const shape = [0.26, 0.42, 0.62, 0.84, 1, 0.84, 0.62, 0.42, 0.26];
  const [v, setV] = useState(0);
  const ref = useRef(0);
  useEffect(() => {
    // perceptual scale: -54 dBFS .. 0 dBFS
    const db = 20 * Math.log10(Math.max(level.peak, 1e-6));
    const target = Math.min(1, Math.max(0, (db + 54) / 54));
    ref.current = target > ref.current ? target : ref.current * 0.86 + target * 0.14;
    setV(ref.current);
  }, [level]);
  return (
    <div className={`meter${off ? ' off' : ''}`} role="meter" aria-label={label} aria-valuemin={0} aria-valuemax={100} aria-valuenow={Math.round(v * 100)}>
      {shape.map((s, i) => (
        <span key={i} style={{ height: `${Math.max(14, Math.round(100 * s * (off ? 0 : v)))}%` }} />
      ))}
    </div>
  );
}

export function Check({ state, children }: { state: 'ok' | 'wait' | 'warn'; children: ReactNode }) {
  return (
    <div className={`check ${state}`} role="status">
      <span className="dot">{state === 'ok' ? Icon.check() : state === 'warn' ? Icon.alert() : null}</span>
      <span className="label">{children}</span>
    </div>
  );
}

export function Switch({ on, onChange, label, small = false }: { on: boolean; onChange: (v: boolean) => void; label: string; small?: boolean }) {
  return <button type="button" role="switch" aria-checked={on} aria-label={label} className={`switch${small ? ' small' : ''}`} onClick={() => onChange(!on)} />;
}

export function Notice({ kind = 'info', title, body, action, onAction }: { kind?: 'info' | 'warn' | 'error'; title: string; body: string; action?: string; onAction?: () => void }) {
  return (
    <div className={`notice ${kind === 'info' ? '' : kind}`} role={kind === 'error' ? 'alert' : 'status'}>
      <div className="text"><h3>{title}</h3><p>{body}</p></div>
      {action && <button className="btn secondary" onClick={onAction} disabled={!onAction}>{action}</button>}
    </div>
  );
}

export function Toast({ text, onDone }: { text: string; onDone: () => void }) {
  useEffect(() => { const id = setTimeout(onDone, 2600); return () => clearTimeout(id); }, [onDone]);
  return <div className="toast" role="status">{text}</div>;
}
