import { useEffect, useRef, useState } from 'react';
import { api, type Instructions as Ins } from '../api';
import { Switch } from '../components';
import { useApp, useT } from '../context';

/** "Instruktioner til Claude": who Claude hears (enforced by the routing)
 *  and how Claude answers (sent to Claude as the first message). */
export function Instructions() {
  const t = useT();
  const { config, update, toast, lang } = useApp();
  const [ins, setIns] = useState<Ins>(config.instructions);
  const [preview, setPreview] = useState('');
  const save = useRef<number | undefined>(undefined);

  // keep the preview current; save typing a moment after it stops
  useEffect(() => {
    api.instructionText(ins, lang, config.claudeHearsDiscord).then(setPreview);
  }, [ins, lang, config.claudeHearsDiscord]);
  const change = (patch: Partial<Ins>, now = true) => {
    const next = { ...ins, ...patch };
    setIns(next);
    window.clearTimeout(save.current);
    if (now) update({ instructions: next });
    else save.current = window.setTimeout(() => update({ instructions: next }), 600);
  };

  const send = async () => {
    try {
      // what is on screen is what gets sent: save pending typing first
      window.clearTimeout(save.current);
      await update({ instructions: ins });
      await api.sendInstructions();
    } catch (e) {
      const key = String(e).replace(/^error\./, '') as keyof typeof t.errors;
      toast((t.errors[key] as string | undefined) ?? t.errors.generic);
    }
  };

  const row = (label: string, on: boolean, set: (v: boolean) => void, extra?: React.ReactNode) => (
    <div className="row">
      <div className="text"><span className="label">{label}</span>{extra}</div>
      <Switch on={on} label={label} onChange={set} />
    </div>
  );

  return (
    <div className="page">
      <header>
        <h1>{t.instructions.title}</h1>
        <p className="muted">{config.externalClaude ? t.instructions.introExternal : t.instructions.intro}</p>
      </header>

      <h2 className="section-title">{t.instructions.hearingTitle}</h2>
      <div className="panel">
        {row(
          t.instructions.hearsOthers,
          config.claudeHearsDiscord,
          (v) => update({ claudeHearsDiscord: v }),
          <span className="small">{config.claudeHearsDiscord ? t.instructions.hearsOthersOn : t.instructions.hearsOthersOff}</span>,
        )}
      </div>

      <h2 className="section-title">{t.instructions.behaviourTitle}</h2>
      <div className="panel">
        <div className="row">
          <div className="text"><span className="label">{t.instructions.onlyWhenCalled}</span></div>
          <Switch on={ins.onlyWhenCalled} label={t.instructions.onlyWhenCalled} onChange={(v) => change({ onlyWhenCalled: v })} />
        </div>
        {ins.onlyWhenCalled && (
          <div className="row field">
            <label className="label" htmlFor="wake">{t.instructions.wakeWord}</label>
            <input id="wake" type="text" value={ins.wakeWord} maxLength={40} onChange={(e) => change({ wakeWord: e.target.value }, false)} />
          </div>
        )}
        {row(t.instructions.short, ins.short, (v) => change({ short: v }))}
        {row(t.instructions.danish, ins.danish, (v) => change({ danish: v }))}
        {row(t.instructions.group, ins.group, (v) => change({ group: v }))}
      </div>

      <h2 className="section-title">{t.instructions.ownTitle}</h2>
      <div className="panel">
        <div className="row field">
          <textarea
            aria-label={t.instructions.ownTitle}
            rows={3}
            maxLength={2000}
            value={ins.own}
            placeholder={t.instructions.ownPlaceholder}
            onChange={(e) => change({ own: e.target.value }, false)}
          />
        </div>
      </div>

      <h2 className="section-title">{t.instructions.sendTitle}</h2>
      <div className="panel">
        <div className="row preview">
          <span className="small">{t.instructions.preview}</span>
          <p className={preview ? 'quote' : 'small'}>{preview || t.instructions.nothing}</p>
          {preview && (
            <button className="btn quiet" style={{ alignSelf: 'flex-start' }} onClick={() => {
              navigator.clipboard.writeText(preview).then(() => toast(t.instructions.copied), () => toast(t.errors.generic));
            }}>{t.instructions.copy}</button>
          )}
        </div>
        {config.externalClaude
          ? <div className="row"><span className="small text">{t.instructions.externalNote}</span></div>
          : <>
            {row(t.instructions.autoSend, ins.autoSend, (v) => change({ autoSend: v }))}
            <div className="row">
              <span className="small text">{t.instructions.note}</span>
              <button className="btn primary" disabled={!preview} onClick={send}>{t.instructions.sendNow}</button>
            </div>
          </>}
      </div>
    </div>
  );
}
