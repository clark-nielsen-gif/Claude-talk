import { useEffect, useRef } from 'react';
import { api } from '../api';
import { useApp, useT } from '../context';
import { fill } from '../i18n';

/** A neutral sketch of a voice-settings panel with the right choice marked.
 *  Deliberately generic: no third-party logos or screenshots. */
function Sketch({ label, choice }: { label: string; choice: string }) {
  return (
    // only the microphone field: big enough to read (the output field is
    // left as it is)
    <svg className="illustration" viewBox="8 10 224 132" role="img" aria-label={label} width="336" height="198">
      <rect x="8" y="10" width="224" height="132" rx="10" fill="var(--sunk)" />
      <rect x="20" y="20" width="96" height="10" rx="5" fill="var(--line-strong)" />
      <rect x="20" y="44" width="200" height="34" rx="8" fill="var(--card)" stroke="var(--line-strong)" />
      <text x="34" y="66" fontSize="13" fontFamily="Inter Variable, sans-serif" fill="var(--ink)">{label}</text>
      <path d="M200 58l6 6 6-6" stroke="var(--ink-2)" strokeWidth="1.8" fill="none" strokeLinecap="round" />
      <rect x="20" y="92" width="200" height="38" rx="8" fill="var(--card)" stroke="var(--clay)" strokeWidth="2" />
      <circle cx="40" cy="111" r="7" fill="var(--clay)" />
      <path d="M36.5 111l2.5 2.5 4.5-5" stroke="#fff" strokeWidth="1.8" fill="none" strokeLinecap="round" />
      <text x="56" y="115" fontSize="13" fontWeight="600" fontFamily="Inter Variable, sans-serif" fill="var(--ink)">{choice}</text>
    </svg>
  );
}

export function DiscordGuide({ mode, onClose, onSettings }: { mode: 'mic' | 'restart'; onClose: () => void; onSettings: () => void }) {
  const t = useT();
  const { toast, status, config } = useApp();
  const first = useRef<HTMLButtonElement>(null);
  const dialog = useRef<HTMLDivElement>(null);
  // the parent passes a new onClose on every status update: read it through
  // a ref, so the dialog is set up once (focus and scroll stay where they are)
  const close = useRef(onClose);
  close.current = onClose;
  useEffect(() => {
    // focus moves into the dialog, stays there, and goes back when it closes
    const opener = document.activeElement as HTMLElement | null;
    // the heading first: what the dialog is about, at its top
    const heading = dialog.current?.querySelector<HTMLElement>('#guide-title');
    if (heading) { heading.tabIndex = -1; heading.focus(); } else first.current?.focus();
    dialog.current?.scrollTo?.({ top: 0 });
    const keys = (e: KeyboardEvent) => {
      if (e.key === 'Escape') close.current();
      if (e.key !== 'Tab' || !dialog.current) return;
      const items = Array.from(dialog.current.querySelectorAll<HTMLElement>('button, [href], input, select, textarea'));
      if (items.length === 0) return;
      const [a, b] = [items[0], items[items.length - 1]];
      const inside = items.includes(document.activeElement as HTMLElement);
      // (from the heading or the text, Tab goes to the first button and
      // Shift+Tab to the last: never to the page behind the dialog)
      if (e.shiftKey && (!inside || document.activeElement === a)) { e.preventDefault(); b.focus(); }
      else if (!e.shiftKey && (!inside || document.activeElement === b)) { e.preventDefault(); a.focus(); }
    };
    window.addEventListener('keydown', keys);
    return () => { window.removeEventListener('keydown', keys); opener?.focus?.(); };
  }, []);
  return (
    <div className="scrim" onClick={(e) => e.target === e.currentTarget && onClose()}>
      <div ref={dialog} className="dialog" role="dialog" aria-modal="true" aria-labelledby="guide-title">
        {mode === 'mic' ? (
          <>
            <h2 id="guide-title">{t.guide.discordTitle}</h2>
            <Sketch label={t.guide.sketchLabel} choice={t.guide.sketchChoice} />
            <ol>{(config.routeDiscord ? t.guide.discordSteps : t.guide.discordStepsRestored).map((s) => <li key={s}>{s}</li>)}</ol>
            {status?.micName && config.routeDiscord && (
              <>
                <p className="muted">{fill(t.guide.micInUse, { mic: status.micName })}</p>
                <button className="btn quiet flush" style={{ alignSelf: 'flex-start' }} onClick={onSettings}>{t.main.otherMic}</button>
              </>
            )}
            <div className="buttons">
              <button className="btn secondary" onClick={onClose}>{t.guide.close}</button>
              <button ref={first} className="btn primary" onClick={() => api.openPlace('discordVoice')}>{t.guide.discordButton}</button>
            </div>
          </>
        ) : (
          <>
            <h2 id="guide-title">{t.guide.restartDiscordTitle}</h2>
            <p className="muted">{t.guide.restartCheckFirst}</p>
            <p className="muted">{t.guide.restartDiscordBody}</p>
            <div className="buttons">
              <button className="btn secondary" onClick={onClose}>{t.guide.close}</button>
              <button className="btn secondary" onClick={() => api.restartDiscord().then(onClose, (e) => { onClose(); toast(String(e).includes('discordClose') ? t.errors.discordClose : t.errors.discordRestart); })}>{t.guide.restartDiscord}</button>
              <button ref={first} className="btn primary" onClick={() => api.openPlace('discordVoice')}>{t.guide.discordButton}</button>
            </div>
          </>
        )}
      </div>
    </div>
  );
}
