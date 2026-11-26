import { useCallback, useEffect, useMemo, useState } from 'react';
import { api, initialScenario, type AppInfo, type AudioStatus, type Config } from './api';
import { TitleBar, Toast } from './components';
import { AppContext, type Ctx } from './context';
import { dictionaries } from './i18n';
import { Advanced } from './screens/Advanced';
import { Help } from './screens/Help';
import { Home } from './screens/Home';
import { Instructions } from './screens/Instructions';
import { Onboarding } from './screens/Onboarding';
import { Settings } from './screens/Settings';

type Page = 'home' | 'instructions' | 'advanced' | 'settings' | 'help';

function useSystemDark(): boolean {
  const q = '(prefers-color-scheme: dark)';
  const [dark, setDark] = useState(() => window.matchMedia(q).matches);
  useEffect(() => {
    const m = window.matchMedia(q);
    const on = () => setDark(m.matches);
    m.addEventListener('change', on);
    return () => m.removeEventListener('change', on);
  }, []);
  return dark;
}

const pageFromScenario = (s: string): Page =>
  s.startsWith('instructions') ? 'instructions' : s.startsWith('advanced') ? 'advanced' : s.startsWith('settings') ? 'settings' : s.startsWith('help') ? 'help' : 'home';

export function App() {
  const [info, setInfo] = useState<AppInfo | null>(null);
  const [config, setConfig] = useState<Config | null>(null);
  const [status, setStatus] = useState<AudioStatus | null>(null);
  const [page, setPage] = useState<Page>(pageFromScenario(initialScenario));
  const [toastText, setToastText] = useState<string | null>(null);
  const closeToast = useCallback(() => setToastText(null), []);
  const systemDark = useSystemDark();

  useEffect(() => {
    api.appInfo().then(setInfo);
    api.getConfig().then(setConfig);
    const a = api.onStatus(setStatus);
    const b = api.onConfig(setConfig);
    return () => { a(); b(); };
  }, []);

  const lang = config?.language ?? 'da';
  const t = dictionaries[lang];
  const dark = config?.theme === 'dark' || (config?.theme === 'system' && systemDark);
  useEffect(() => {
    document.documentElement.dataset.theme = dark ? 'dark' : 'light';
    document.documentElement.lang = lang;
  }, [dark, lang]);

  const update = useCallback(async (patch: Partial<Config>) => {
    try {
      setConfig(await api.setConfig(patch));
    } catch (e) {
      const key = String(e).replace(/^error\./, '') as keyof typeof t.errors;
      setToastText(t.errors[key] ?? t.errors.internal);
      // part of the change may have been saved (a shortcut that is taken):
      // show what the settings really are now
      api.getConfig().then(setConfig).catch(() => {});
    }
  }, [t]);

  const ctx: Ctx | null = useMemo(
    () => (config ? { lang, config, status, update, toast: setToastText } : null),
    [lang, config, status, update],
  );

  if (!info || !ctx || !config) return <div className="shell"><TitleBarless /></div>;

  const onboarding = !config.onboardingDone;
  const initialStep = initialScenario === 'onboarding-setup' || initialScenario === 'onboarding-cable-missing' || initialScenario === 'onboarding-mic-blocked' ? 'setup' : initialScenario === 'onboarding-ready' ? 'ready' : 'welcome';

  return (
    <AppContext.Provider value={ctx}>
      <div className="shell">
        <TitleBar />
        {onboarding ? (
          <Onboarding info={info} initial={initialStep} onDone={() => update({ onboardingDone: true })} />
        ) : (
          <div className="body">
            <nav className="nav" aria-label={t.appName}>
              {(['home', 'instructions', 'settings', 'help', 'advanced'] as Page[]).map((p) => (
                <button key={p} aria-current={page === p ? 'page' : undefined} onClick={() => setPage(p)}>
                  {t.nav[p]}
                </button>
              ))}
            </nav>
            <main className="content">
              {page === 'home' && <Home info={info} onSettings={() => setPage('settings')} />}
              {page === 'instructions' && <Instructions />}
              {page === 'advanced' && <Advanced info={info} />}
              {page === 'settings' && <Settings />}
              {page === 'help' && <Help info={info} />}
            </main>
          </div>
        )}
        {toastText && <Toast key={toastText} text={toastText} onDone={closeToast} closeLabel={t.notices.dismiss} />}
      </div>
    </AppContext.Provider>
  );
}

function TitleBarless() {
  return <div className="titlebar" data-tauri-drag-region />;
}
