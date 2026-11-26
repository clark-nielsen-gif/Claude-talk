import { useCallback, useEffect, useState } from 'react';
import { api, type Matrix } from '../api';
import { Switch } from '../components';
import { useApp, useT } from '../context';
import { fill } from '../i18n';

/** Sources × outputs, a volume per cell. Echo cells are locked, loops refused. */
export function Advanced() {
  const t = useT();
  const { config, update } = useApp();
  const [m, setM] = useState<Matrix | null>(null);
  const reload = useCallback(() => { api.matrix().then(setM); }, []);
  useEffect(reload, [reload, config.advancedUnlocked]);

  if (!m) return null;
  const cell = (s: number, o: number) => m.cells.find((c) => c.source === s && c.output === o);

  return (
    <div className="page">
      <header>
        <h1>{t.advanced.title}</h1>
        <p className="muted">{t.advanced.intro}</p>
      </header>
      <div className="panel">
        <table className="matrix">
          <thead>
            <tr>
              <th scope="col">{t.advanced.sources}</th>
              {m.outputs.map((o) => <th key={o.id} scope="col">{o.label}</th>)}
            </tr>
          </thead>
          <tbody>
            {m.sources.map((s) => (
              <tr key={s.id}>
                <th scope="row">{s.label}</th>
                {m.outputs.map((o) => {
                  const c = cell(s.id, o.id);
                  if (!c) return <td key={o.id} />;
                  if (c.state === 'locked' || c.state === 'loop') {
                    return (
                      <td key={o.id}>
                        <div className="cell locked">{c.state === 'loop' ? t.advanced.loop : fill(t.advanced.locked, { who: c.who ?? '' })}</div>
                      </td>
                    );
                  }
                  const on = c.state === 'on';
                  const label = `${s.label} → ${o.label}`;
                  return (
                    <td key={o.id}>
                      <div className="cell">
                        <Switch small on={on} label={label} onChange={async (v) => { await api.setRoute(s.id, o.id, v, c.gainDb); reload(); }} />
                        {on ? (
                          <>
                            <input
                              type="range" min={-30} max={6} step={1} value={c.gainDb}
                              aria-label={`${t.advanced.gain}: ${label}`}
                              onChange={async (e) => { await api.setRoute(s.id, o.id, true, Number(e.target.value)); reload(); }}
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
