import { useState } from 'react';
import { api } from './api';
import { useApp, useT } from './context';

/** "Installer Claude Talk-mikrofonen": one Windows prompt, then a clear result. */
export function useInstallCable(): { installing: boolean; install: () => void } {
  const t = useT();
  const { toast } = useApp();
  const [installing, setInstalling] = useState(false);
  const install = () => {
    setInstalling(true);
    api.installCable()
      .then((code) => {
        if (code === 2) toast(t.errors.cableNoInternet);
        else if (code === 6) toast(t.errors.cableNoArm);
        else if (code === 7) toast(t.errors.cableBrokenBody);
        else if (code !== 0) toast(t.errors.cableInstallFailed);
      })
      .catch((e) => {
        if (String(e) === 'error.cableNeedsInstaller') toast(t.errors.cableNeedsInstaller);
        else if (String(e) !== 'cancelled') toast(t.errors.cableInstallFailed);
      })
      .finally(() => setInstalling(false));
  };
  return { installing, install };
}
