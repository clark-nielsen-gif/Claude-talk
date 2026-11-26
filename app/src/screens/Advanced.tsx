import { useCallback, useEffect, useState, type FormEvent } from 'react';
import { api, type AppInfo, type Labeled, type Matrix, type MixerChoices } from '../api';
import { Icon, Switch } from '../components';
import { useApp, useT } from '../context';
import { fill } from '../i18n';

/** Picks a new source (input device or program) or output (playback device). */
function Adder({ kind, onDone }: { kind: 'source' | 'output'; onDone: (added: boolean) => void }) {
  const t = useT();
  const { toast } = useApp();
  const [choices, setChoices] = useState<MixerChoices | null>(null);
  const [pick, setPick] = useState('');
  const [name, setName] = useState('');
  const [busy, setBusy] = useState(false);
  useEffect(() => { api.mixerChoices().then(setChoices); }, []);
  if (!choices) return null;

  const submit = async (e: FormEvent) => {
    e.preventDefault();
    if (!pick || busy) return;
    const at = pick.indexOf(':');
    const [type, value] = [pick.slice(0, at), pick.slice(at + 1)];
    const label = name.trim() || (kind === 'source'
      ? [...choices.inputs, ...choices.programs].find((c) => c.value === value)?.label ?? ''
      : choices.outputs.find((c) => c.value === value)?.label ?? '');
    setBusy(true);
    try {
      if (kind === 'source') await api.addMixerSource(type as 'input' | 'program', value, label);
      else await api.addMixerOutput(value, label);
      onDone(true);
    } catch {
      toast(t.errors.generic);
    } finally {
      setBusy(false);
    }
  };

  const title = kind === 'source' ? t.advanced.pickSource : t.advanced.pickOutput;
  return (
    <form className="adder" onSubmit={submit} aria-label={title}>
      <select value={pick} onChange={(e) => setPick(e.target.value)} aria-label={title} required>
        <option value="" disabled>{title}</option>
        {kind === 'source' ? (
          <>
            <optgroup label={t.advanced.inputsGroup}>
              {choices.inputs.map((c) => <option key={c.value} value={`input:${c.value}`}>{c.label}</option>)}
            </optgroup>
            <optgroup label={t.advanced.programsGroup}>
              {choices.programs.map((c) => <option key={c.value} value={`program:${c.value}`}>{c.label}</option>)}
            </optgroup>
          </>
        ) : choices.outputs.map((c) => <option key={c.value} value={`output:${c.value}`}>{c.label}</option>)}
      </select>
      <input type="text" value={name} maxLength={40} placeholder={t.advanced.nameOptional} aria-label={t.advanced.nameOptional} onChange={(e) => setName(e.target.value)} />
      <button type="submit" className="btn primary" disabled={!pick || busy}>{t.advanced.add}</button>
      <button type="button" className="btn quiet" onClick={() => onDone(false)}>{t.advanced.cancel}</button>
      {kind === 'source' && choices.programs.length === 0 && <p className="small hint">{t.advanced.noPrograms}</p>}
    </form>
  );
}

function RemoveButton({ item, onRemove }: { item: Labeled; onRemove: () => void }) {
  const t = useT();
  if (!item.removable) return null;
  const label = fill(t.advanced.remove, { name: item.label });
  return <button className="remove" onClick={onRemove} aria-label={label} title={label}>{Icon.close()}</button>;
}

/** Claude Talk's own sources and outputs, named in the UI's language. */
const builtIn: Record<number, { da: string; en: string }> = {
  1: { da: 'Mikrofon', en: 'Microphone' },
  101: { da: 'Til Discord', en: 'To Discord' },
  102: { da: 'Til Claude', en: 'To Claude' },
  103: { da: 'Lyt med', en: 'Listen in' },
  190: { da: 'Til Claude (kabel)', en: 'To Claude (cable)' },
};

/** Sources × outputs, a volume per cell. Echo cells are locked, loops refused. */
export function Advanced({ info }: { info: AppInfo }) {
  const t = useT();
  const { config, update, toast, lang, setConfig } = useApp();
  const name = (item: Labeled) => {
    if (item.removable) return item.label;
    // Claude in another program: "Claude-appen", "Claude i Chrome", "Claude i Edge"
    if (item.id === 191 && lang === 'en') return item.label === 'Claude-appen' ? 'The Claude app' : item.label.replace(' i ', ' in ');
    return builtIn[item.id]?.[lang] ?? item.label;
  };
  // every change: an error is shown, and the table shows what really is
  const act = (f: () => Promise<unknown>) => async () => {
    try { await f(); } catch { toast(t.errors.generic); } finally { reload(); }
  };
  // the mode in force, from the settings (every page reads the same)
  const external = config.externalClaude ?? null;
  const chooseExternal = async (v: string) => {
    const exe = v === 'off' ? null : v;
    try {
      await api.setExternalClaude(exe);
    } catch {
      toast(t.errors.generic);
    } finally {
      setConfig(await api.getConfig());
      reload();
    }
  };
  const seg = (value: string, options: [string, string][], set: (v: string) => void, label: string) => (
    <div className="segmented" role="group" aria-label={label}>
      {options.map(([v, l]) => <button key={v} aria-pressed={value === v} onClick={() => set(v)}>{l}</button>)}
    </div>
  );
  const [m, setM] = useState<Matrix | null>(null);
  const [adding, setAdding] = useState<null | 'source' | 'output'>(null);
  const [presets, setPresets] = useState(config.presets ?? []);
  const [presetName, setPresetName] = useState('');
  const [confirmDelete, setConfirmDelete] = useState<string | null>(null);
  const reload = useCallback(() => { api.matrix().then(setM); }, []);
  useEffect(reload, [reload, config.advancedUnlocked]);
  const reloadPresets = () => api.getConfig().then((c) => setPresets(c.presets ?? []));

  if (!m) return null;
  const cell = (s: number, o: number) => m.cells.find((c) => c.source === s && c.output === o);

  const savePreset = async (e: FormEvent) => {
    e.preventDefault();
    try {
      await api.savePreset(presetName);
      setPresetName('');
      toast(t.advanced.presetSaved);
      reloadPresets();
    } catch {
      toast(t.errors.presetName);
    }
  };
  const loadPreset = (preset: string) => act(() => api.loadPreset(preset))();

  return (
    <div className="page">
      <header>
        <h1>{t.advanced.title}</h1>
        <p className="muted">{t.advanced.intro}</p>
      </header>
      <div className="panel">
        <div className="matrix-scroll">
        <table className="matrix">
          <thead>
            <tr>
              <th scope="col">{t.advanced.sources}</th>
              {m.outputs.map((o) => (
                <th key={o.id} scope="col">
                  <span className="headname"><span className="name">{name(o)}</span><RemoveButton item={o} onRemove={act(() => api.removeMixerOutput(o.id))} /></span>
                </th>
              ))}
            </tr>
          </thead>
          <tbody>
            {m.sources.map((s) => (
              <tr key={s.id}>
                <th scope="row">
                  <span className="headname"><span className="name">{name(s)}</span><RemoveButton item={s} onRemove={act(() => api.removeMixerSource(s.id))} /></span>
                </th>
                {m.outputs.map((o) => {
                  const c = cell(s.id, o.id);
                  if (!c) return <td key={o.id} />;
                  if (c.state === 'locked' || c.state === 'loop') {
                    const why = c.state === 'loop' ? t.advanced.loop : c.who === 'Du' ? t.advanced.lockedYou : fill(t.advanced.locked, { who: c.who ?? '' });
                    return (
                      <td key={o.id}>
                        <div className="cell locked" title={why}>
                          {c.state === 'loop' ? t.advanced.loopShort : t.advanced.lockedShort}
                          <span className="sr-only">{`: ${why}`}</span>
                        </div>
                      </td>
                    );
                  }
                  const on = c.state === 'on';
                  const label = `${name(s)} → ${name(o)}`;
                  return (
                    <td key={o.id}>
                      <div className="cell">
                        <Switch small on={on} label={label} onChange={(v) => act(() => api.setRoute(s.id, o.id, v, c.gainDb))()} />
                        {on ? (
                          <>
                            <input
                              type="range" min={-30} max={6} step={1} value={c.gainDb}
                              aria-label={`${t.advanced.gain}: ${label}`}
                              aria-valuetext={`${c.gainDb} dB`}
                              onChange={(e) => act(() => api.setRoute(s.id, o.id, true, Number(e.target.value)))()}
                            />
                            <span className="small gain">{`${c.gainDb > 0 ? '+' : ''}${c.gainDb} dB`}</span>
                          </>
                        ) : <span className="small">{t.advanced.off}</span>}
                      </div>
                    </td>
                  );
                })}
              </tr>
            ))}
          </tbody>
        </table>
        </div>
        <div className="matrix-actions">
          {adding
            ? <Adder kind={adding} onDone={(added) => { setAdding(null); if (added) reload(); }} />
            : (
              <>
                <button className="btn secondary" onClick={() => setAdding('source')}>{t.advanced.addSource}</button>
                <button className="btn secondary" onClick={() => setAdding('output')}>{t.advanced.addOutput}</button>
              </>
            )}
        </div>
      </div>

      <div className="panel">
        <div className="row">
          <div className="text">
            <span className="label">{t.advanced.presets}</span>
            <span className="small">{t.advanced.presetsIntro}</span>
          </div>
        </div>
        <div className="row presets">
          <button className="btn secondary" onClick={() => loadPreset('standard')}>{t.advanced.standardPreset}</button>
          {presets.map((p) => {
            const del = fill(t.advanced.deletePreset, { name: p.name });
            return (
              <span key={p.name} className="preset">
                <button className="btn secondary" onClick={() => loadPreset(p.name)}>{p.name}</button>
                <button className="remove" aria-label={del} title={del} onClick={async () => {
                  if (confirmDelete !== p.name) { setConfirmDelete(p.name); toast(fill(t.advanced.deletePresetAgain, { name: p.name })); return; }
                  setConfirmDelete(null);
                  try { await api.deletePreset(p.name); } catch { toast(t.errors.generic); }
                  reloadPresets();
                }}>{Icon.close()}</button>
              </span>
            );
          })}
        </div>
        <form className="row adder" onSubmit={savePreset}>
          <input type="text" value={presetName} maxLength={40} placeholder={t.advanced.presetName} aria-label={t.advanced.presetName} onChange={(e) => setPresetName(e.target.value)} />
          <button type="submit" className="btn secondary" disabled={!presetName.trim()}>{t.advanced.savePreset}</button>
        </form>
      </div>

      {info.extraCable && (
        <>
          <h2 className="section-title">{t.settings.externalTitle}</h2>
          <div className="panel">
            <div className="row">
              <div className="text">
                <span className="label">{t.settings.externalLabel}</span>
                <span className="small">{fill(t.settings.externalBody, { cable: info.extraCable.name, mic: info.extraCable.output ?? info.extraCable.name })}</span>
              </div>
            </div>
            <div className="row">
              {seg(external ?? 'off', [['off', t.settings.externalOff], ['claude.exe', t.settings.externalApp], ['chrome.exe', 'Chrome'], ['msedge.exe', 'Edge']], chooseExternal, t.settings.externalLabel)}
            </div>
            {external && (
              <div className="row">
                <span className="small">{fill(t.settings.externalHow, { mic: info.extraCable.output ?? info.extraCable.name })}</span>
              </div>
            )}
          </div>
        </>
      )}


      <div className="panel">
        <div className="row">
          <div className="text">
            <span className="label">{t.advanced.unlock}</span>
            <span className="small">{t.advanced.unlockWarning}</span>
          </div>
          <Switch on={config.advancedUnlocked} label={t.advanced.unlock} onChange={async (v) => { await api.setUnlocked(v); await update({ advancedUnlocked: v }); }} />
        </div>
      </div>
    </div>
  );
}
