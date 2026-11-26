import { useEffect, useState } from 'react';
import { api, type EchoMode, type Hotkeys, type Theme } from '../api';
import { Switch } from '../components';
import { useApp, useT } from '../context';
import { fill } from '../i18n';

/** Records a key combination like "Ctrl+Shift+D". Only one listens at a time. */
function KeyPicker({ value, onChange, label, listening, setListening }: {
  value: string; onChange: (v: string) => void; label: string; listening: boolean; setListening: (on: boolean) => void;
}) {
  const t = useT();
  const [refused, setRefused] = useState(false);
  useEffect(() => {
    if (!listening) { setRefused(false); return; }
    const down = (e: KeyboardEvent) => {
      // plain Tab, Enter and Space keep working (to reach and press Fortryd)
      if (!e.ctrlKey && !e.metaKey && ['Tab', 'Enter', ' '].includes(e.key)) return;
      e.preventDefault();
      if (e.key === 'Escape') { setListening(false); return; }
      if (['Control', 'Alt', 'Shift', 'Meta'].includes(e.key)) return;
      const mods = [e.ctrlKey && 'Ctrl', e.altKey && 'Alt', e.shiftKey && 'Shift', e.metaKey && 'Super'].filter(Boolean) as string[];
      // A global shortcut takes the keys from every program: Shift+M would
      // swallow every capital M, Alt+letter opens menus, Ctrl+Alt is AltGr
      // on Danish keyboards (@, £, $, €), and Alt+F4 closes windows.
      const ok = (e.ctrlKey || e.metaKey) && !(e.ctrlKey && e.altKey && !e.metaKey);
      if (!ok) { setRefused(true); return; }
      // letters, digits, F-keys and Space only: other keys (Æ, Ø, Å, "+")
      // cannot be registered as global shortcuts reliably
      const key = e.code.startsWith('Key') ? e.code.slice(3) : e.code.startsWith('Digit') ? e.code.slice(5) : e.code === 'Space' ? 'Space' : /^F\d{1,2}$/.test(e.code) && e.code !== 'F12' ? e.code : null;
      if (!key) return;
      onChange([...mods, key].join('+'));
      setListening(false);
    };
    window.addEventListener('keydown', down, true);
    return () => window.removeEventListener('keydown', down, true);
  }, [listening, onChange]);
  return (
    <div style={{ display: 'flex', alignItems: 'center', gap: '0.75rem' }}>
      <span className="kbd" aria-live="polite">{listening ? (refused ? t.settings.keyNeedsCtrl : t.settings.pressKeys) : value}</span>
      {listening
        ? <button className="btn quiet" onClick={() => setListening(false)}>{t.settings.cancel}</button>
        : <button className="btn quiet" aria-label={`${t.settings.change}: ${label}`} onClick={() => setListening(true)}>{t.settings.change}</button>}
    </div>
  );
}

/** Which microphone is "you" (Windows' default, or one the user picks). */
function MicPicker() {
  const t = useT();
  const { config, status, toast, setConfig } = useApp();
  const [mics, setMics] = useState<{ id: string; name: string }[]>([]);
  useEffect(() => { api.listMicrophones().then(setMics).catch(() => {}); }, [status?.micName]);
  const choose = async (id: string) => {
    try { setConfig(await api.setMicrophone(id || null)); } catch { toast(t.errors.generic); }
  };
  const chosen = config.microphone ?? '';
  const known = !chosen || mics.some((m) => m.id === chosen);
  return (
    <select id="mic-select" aria-label={t.settings.microphone} value={chosen} onChange={(e) => choose(e.target.value)} className="field">
      <option value="">{!chosen && status?.micName ? `${t.settings.micDefault}: ${status.micName}` : t.settings.micDefault}</option>
      {mics.map((m) => <option key={m.id} value={m.id}>{m.name}</option>)}
      {!known && <option value={chosen}>{fill(t.settings.micUnplugged, { name: config.microphoneName ?? '' })}</option>}
    </select>
  );
}

export function Settings() {
  const t = useT();
  const { config, update, toast } = useApp();
  const [listening, setListening] = useState<string | null>(null);
  const [confirmRestore, setConfirmRestore] = useState(false);
  useEffect(() => {
    if (!confirmRestore) return;
    const id = setTimeout(() => setConfirmRestore(false), 6000);
    return () => clearTimeout(id);
  }, [confirmRestore]);
  const picker = (key: keyof Hotkeys, label: string) => (
    <KeyPicker label={label} value={config.hotkeys[key]} onChange={(v) => hk({ [key]: v })}
      listening={listening === key} setListening={(on) => setListening(on ? key : null)} />
  );

  const hk = (patch: Partial<Hotkeys>) => update({ hotkeys: { ...config.hotkeys, ...patch } });
  const seg = <T extends string>(value: T, options: [T, string][], set: (v: T) => void, label: string) => (
    <div className="segmented" role="group" aria-label={label}>
      {options.map(([v, l]) => <button key={v} aria-pressed={value === v} onClick={() => set(v)}>{l}</button>)}
    </div>
  );

  return (
    <div className="page">
      <header><h1>{t.settings.title}</h1></header>

      <h2 className="section-title">{t.settings.appearance}</h2>
      <div className="panel">
        <div className="row">
          <div className="text"><span className="label">{t.settings.theme}</span></div>
          {seg<Theme>(config.theme, [['system', t.settings.themeSystem], ['light', t.settings.themeLight], ['dark', t.settings.themeDark]], (v) => update({ theme: v }), t.settings.theme)}
        </div>
        <div className="row">
          <div className="text"><span className="label">{t.settings.language}</span></div>
          {seg<'da' | 'en'>(config.language, [['da', 'Dansk'], ['en', 'English']], (v) => update({ language: v }), t.settings.language)}
        </div>
      </div>

      <div className="panel">
        <div className="row">
          <div className="text"><span className="label">{t.settings.microphone}</span><span className="small">{config.routeDiscord ? t.settings.microphoneHint : t.settings.microphoneHintRestored}</span></div>
          <MicPicker />
        </div>
      </div>

      <div className="panel">
        <div className="row">
          <div className="text"><span className="label">{t.settings.startup}</span><span className="small">{t.settings.startupHint}</span></div>
          <Switch on={config.autostart} label={t.settings.startup} onChange={(v) => update({ autostart: v })} />
        </div>
        <div className="row">
          <div className="text"><span className="label">{t.settings.echo}</span></div>
          {seg<EchoMode>(config.echo, [['auto', t.settings.echoAuto], ['on', t.settings.echoOn], ['off', t.settings.echoOff]], (v) => update({ echo: v }), t.settings.echo)}
        </div>
      </div>

      <h2 className="section-title">{t.settings.hotkeys}</h2>
      <div className="panel">
        <div className="row"><div className="text"><span className="label">{t.settings.hkMuteClaude}</span></div>{picker('muteClaudeToDiscord', t.settings.hkMuteClaude)}</div>
        <div className="row"><div className="text"><span className="label">{t.settings.hkMuteMic}</span></div>{picker('muteMic', t.settings.hkMuteMic)}</div>
        <div className="row"><div className="text"><span className="label">{t.settings.hkPtt}</span></div>{picker('pushToTalkClaude', t.settings.hkPtt)}</div>
        <div className="row">
          <div className="text"><span className="label">{t.settings.pttEnable}</span><span className="small">{t.settings.pttEnableHint}</span></div>
          <Switch on={config.pushToTalk} label={t.settings.pttEnable} onChange={(v) => update({ pushToTalk: v })} />
        </div>
      </div>

      <div className="panel">
        <div className="row">
          <div className="text"><span className="label">{t.settings.restore}</span><span className="small">{t.settings.restoreHint}</span></div>
          <div className="confirm" aria-live="polite">
            {confirmRestore && <span className="small">{t.settings.restoreAsk}</span>}
            {confirmRestore && <button className="btn quiet" onClick={() => setConfirmRestore(false)}>{t.settings.restoreNo}</button>}
            <button className={confirmRestore ? 'btn primary' : 'btn secondary'} onClick={async () => {
              if (!confirmRestore) { setConfirmRestore(true); return; }
              setConfirmRestore(false);
              try { await api.restoreAudioSettings(); toast(t.settings.restored); } catch { toast(t.errors.generic); }
            }}>{confirmRestore ? t.settings.restoreSure : t.settings.restore}</button>
          </div>
        </div>
      </div>
    </div>
  );
}
