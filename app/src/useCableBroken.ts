import { useEffect, useState } from 'react';
import type { AppInfo, AudioStatus } from './api';

/**
 * The cable is installed but not working (turned off in Windows' sound
 * settings, either side): true after 5 s, not for a blip. Shared by every
 * screen that says whether the sound works, so they never disagree.
 */
export function useCableBroken(status: AudioStatus | null, info: AppInfo): boolean {
  const [since, setSince] = useState<number | null>(null);
  useEffect(() => {
    if (!status || status.cableOk || !info.cableInstalled) setSince(null);
    else setSince((v) => v ?? Date.now());
  }, [status, info.cableInstalled]);
  const [now, setNow] = useState(Date.now());
  useEffect(() => {
    if (since === null) return;
    const id = setInterval(() => setNow(Date.now()), 1000);
    return () => clearInterval(id);
  }, [since]);
  return since !== null && now - since > 5000;
}
