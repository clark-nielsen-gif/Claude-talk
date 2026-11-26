import { useEffect, useState } from 'react';
import { api, type EchoMode, type Hotkeys, type Theme } from '../api';
import { Switch } from '../components';
import { useApp, useT } from '../context';

/** Records a key combination like "Ctrl+Alt+D". */
function KeyPicker({ value, onChange }: { value: string; onChange: (v: string) => void }) {
  const t = useT();
  const [listening, setListening] = useState(false);
  useEffect(() => {
    if (!listening) return;
    const down = (e: KeyboardEvent) => {
      e.preventDefault();
      if (e.key === 'Escape') { setListening(false); return; }
      if (['Control', 'Alt', 'Shift', 'Meta'].includes(e.key)) return;
      const mods = [e.ctrlKey && 'Ctrl', e.altKey && 'Alt', e.shiftKey && 'Shift', e.metaKey && 'Super'].filter(Boolean) as string[];
      if (mods.length === 0) return; // a global shortcut needs a modifier
      // letters, digits, F-keys and Space only: other keys (Æ, Ø, Å, "+")
      // cannot be registered as global shortcuts reliably
      const key = e.code.startsWith('Key') ? e.code.slice(3) : e.code.startsWith('Digit') ? e.code.slice(5) : e.code === 'Space' ? 'Space' : /^F\d{1,2}$/.test(e.code) ? e.code : null;
      if (!key) return;
      onChange([...mods, key].join('+'));
      setListening(false);
    };
    window.addEventListener('keydown', down, true);
    return () => window.removeEventListener('keydown', down, true);
  }, [listening, onChange]);
  return (
    <div style={{ display: 'flex', alignItems: 'center', gap: '0.75rem' }}>
      <span className="kbd">{listening ? t.settings.pressKeys : value}</span>
      <button className="btn quiet" onClick={() => setListening(true)}>{t.settings.change}</button>
    </div>
  );
}

export function Settings() {
  const t = useT();
  const { config, update, toast } = useApp();

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
        <div className="row"><div className="text"><span className="label">{t.settings.hkMuteClaude}</span></div><KeyPicker value={config.hotkeys.muteClaudeToDiscord} onChange={(v) => hk({ muteClaudeToDiscord: v })} /></div>
        <div className="row"><div className="text"><span className="label">{t.settings.hkMuteMic}</span></div><KeyPicker value={config.hotkeys.muteMic} onChange={(v) => hk({ muteMic: v })} /></div>
        <div className="row"><div className="text"><span className="label">{t.settings.hkPtt}</span></div><KeyPicker value={config.hotkeys.pushToTalkClaude} onChange={(v) => hk({ pushToTalkClaude: v })} /></div>
        <div className="row">
          <div className="text"><span className="label">{t.settings.pttEnable}</span></div>
          <Switch on={config.pushToTalk} label={t.settings.pttEnable} onChange={(v) => update({ pushToTalk: v })} />
        </div>
      </div>

      <div className="panel">
        <div className="row">
          <div className="text"><span className="label">{t.settings.restore}</span><span className="small">{t.settings.restoreHint}</span></div>
          <button className="btn secondary" onClick={async () => { await api.restoreAudioSettings(); toast(t.settings.restored); }}>{t.settings.restore}</button>
        </div>
      </div>
    </div>
  );
}
