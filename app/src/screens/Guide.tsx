import { useEffect, useRef } from 'react';
import { api } from '../api';
import { useT } from '../context';

/** A neutral sketch of a voice-settings panel with the right choice marked.
 *  Deliberately generic: no third-party logos or screenshots. */
function Sketch({ label }: { label: string }) {
  return (
    <svg className="illustration" viewBox="0 0 480 150" role="img" aria-label={label} width="100%">
      <rect x="0" y="0" width="480" height="150" rx="12" fill="var(--sunk)" />
      <rect x="20" y="20" width="96" height="10" rx="5" fill="var(--line-strong)" />
      <rect x="20" y="44" width="200" height="34" rx="8" fill="var(--card)" stroke="var(--line-strong)" />
      <text x="34" y="66" fontSize="13" fontFamily="Inter Variable, sans-serif" fill="var(--ink)">{label}</text>
      <path d="M200 58l6 6 6-6" stroke="var(--ink-2)" strokeWidth="1.8" fill="none" strokeLinecap="round" />
      <rect x="20" y="92" width="200" height="38" rx="8" fill="var(--card)" stroke="var(--clay)" strokeWidth="2" />
      <circle cx="40" cy="111" r="7" fill="var(--clay)" />
      <path d="M36.5 111l2.5 2.5 4.5-5" stroke="#fff" strokeWidth="1.8" fill="none" strokeLinecap="round" />
      <text x="56" y="115" fontSize="13" fontWeight="600" fontFamily="Inter Variable, sans-serif" fill="var(--ink)">Claude Talk – til Discord</text>
      <rect x="252" y="20" width="96" height="10" rx="5" fill="var(--line-strong)" />
      <rect x="252" y="44" width="208" height="34" rx="8" fill="var(--card)" stroke="var(--line)" />
      <rect x="266" y="58" width="120" height="6" rx="3" fill="var(--line-strong)" />
    </svg>
  );
}

export function DiscordGuide({ mode, onClose }: { mode: 'mic' | 'restart'; onClose: () => void }) {
  const t = useT();
  const first = useRef<HTMLButtonElement>(null);
  useEffect(() => {
    first.current?.focus();
    const esc = (e: KeyboardEvent) => e.key === 'Escape' && onClose();
    window.addEventListener('keydown', esc);
    return () => window.removeEventListener('keydown', esc);
  }, [onClose]);
  return (
    <div className="scrim" onClick={(e) => e.target === e.currentTarget && onClose()}>
      <div className="dialog" role="dialog" aria-modal="true" aria-labelledby="guide-title">
        {mode === 'mic' ? (
          <>
            <h2 id="guide-title">{t.guide.discordTitle}</h2>
            <Sketch label="Inputenhed" />
            <ol>{t.guide.discordSteps.map((s) => <li key={s}>{s}</li>)}</ol>
            <div className="buttons">
              <button className="btn secondary" onClick={onClose}>{t.guide.close}</button>
              <button ref={first} className="btn primary" onClick={() => api.openPlace('discordVoice')}>{t.guide.discordButton}</button>
            </div>
          </>
        ) : (
          <>
            <h2 id="guide-title">{t.guide.restartDiscordTitle}</h2>
            <p className="muted">{t.guide.restartDiscordBody}</p>
            <div className="buttons">
              <button className="btn secondary" onClick={onClose}>{t.guide.close}</button>
              <button ref={first} className="btn primary" onClick={async () => { await api.restartDiscord(); onClose(); }}>{t.guide.restartDiscord}</button>
            </div>
          </>
        )}
      </div>
    </div>
  );
}
