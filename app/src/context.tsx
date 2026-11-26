import { createContext, useContext } from 'react';
import { dictionaries, type Lang, type Texts } from './i18n';
import type { AudioStatus, Config } from './api';

export interface Ctx {
  lang: Lang;
  config: Config;
  status: AudioStatus | null;
  update: (patch: Partial<Config>) => Promise<void>;
  toast: (text: string) => void;
}

export const AppContext = createContext<Ctx | null>(null);

export function useApp(): Ctx {
  const c = useContext(AppContext);
  if (!c) throw new Error('AppContext missing');
  return c;
}

export function useT(): Texts {
  const c = useContext(AppContext);
  return dictionaries[c?.lang ?? 'da'];
}
