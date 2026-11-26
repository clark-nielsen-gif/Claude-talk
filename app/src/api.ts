// Talks to the Rust side. Outside Tauri (only in the screenshot tests) a
// scenario backend renders each screen and error state with fixed data.
import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';

export type Theme = 'system' | 'light' | 'dark';
export type EchoMode = 'auto' | 'on' | 'off';

export interface Hotkeys {
  muteClaudeToDiscord: string;
  muteMic: string;
  pushToTalkClaude: string;
}

export interface Config {
  version: number;
  language: 'da' | 'en';
  theme: Theme;
  autostart: boolean;
  onboardingDone: boolean;
  claudeInDiscord: boolean;
  micMuted: boolean;
  claudeToDiscordMuted: boolean;
  pushToTalk: boolean;
  echo: EchoMode;
  hotkeys: Hotkeys;
  advancedUnlocked: boolean;
}

export interface Level { peak: number; rms: number }

export interface AudioStatus {
  engineRunning: boolean;
  mic: Level; claude: Level; discord: Level; toDiscord: Level; toClaude: Level;
  micState: 'ok' | 'missing' | 'blocked' | 'starting' | '';
  micName: string;
  claudeWindowOpen: boolean;
  claudeListening: boolean;
  discordRunning: boolean;
  /** 'yes' Discord records our cable, 'no' it records another mic, '' unknown */
  discordUsesCable: 'yes' | 'no' | '';
  discordNeedsRestart: boolean;
  speakers: boolean;
  echoActive: boolean;
  cableOk: boolean;
  latencyMs: number;
  driftPpm: number;
  xruns: number;
}

export interface AppInfo {
  version: string;
  windowsBuild: number;
  supported: boolean;
  cableInstalled: boolean;
  testMode: boolean;
}

export interface Route { source: number; output: number; gainDb: number; muted: boolean }
export interface MatrixCell { source: number; output: number; state: 'on' | 'off' | 'locked' | 'loop'; gainDb: number; who?: string }
export interface Matrix { sources: { id: number; label: string }[]; outputs: { id: number; label: string }[]; cells: MatrixCell[] }
export interface SelfTestResult { route: string; passed: boolean }

const inTauri = typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window;

// ---- scenario backend (screenshots only) ----------------------------------
const params = new URLSearchParams(typeof location !== 'undefined' ? location.search : '');
const scenario = params.get('scenario') ?? 'main-ok';

const defaultConfig: Config = {
  version: 1, language: (params.get('lang') as 'da' | 'en') ?? 'da', theme: (params.get('theme') as Theme) ?? 'light',
  autostart: true, onboardingDone: !scenario.startsWith('onboarding'), claudeInDiscord: scenario !== 'main-off',
  micMuted: scenario === 'main-muted', claudeToDiscordMuted: false, pushToTalk: false, echo: 'auto',
  hotkeys: { muteClaudeToDiscord: 'Ctrl+Alt+D', muteMic: 'Ctrl+Alt+M', pushToTalkClaude: 'Ctrl+Alt+Space' },
  advancedUnlocked: false,
};

function scenarioStatus(t: number): AudioStatus {
  const wave = (f: number, a: number) => Math.max(0, a * (0.55 + 0.45 * Math.sin(t * f)));
  const lv = (a: number, f = 7) => ({ peak: wave(f, a), rms: wave(f, a) * 0.6 });
  const ok: AudioStatus = {
    engineRunning: true, mic: lv(0.5), claude: lv(0.6, 5), discord: lv(0.35, 9), toDiscord: lv(0.6, 6), toClaude: lv(0.45, 8),
    micState: 'ok', micName: 'Headset-mikrofon (Jabra Evolve2)', claudeWindowOpen: true, claudeListening: true,
    discordRunning: true, discordUsesCable: 'yes', discordNeedsRestart: false, speakers: false, echoActive: false, cableOk: true,
    latencyMs: 31, driftPpm: -12, xruns: 0,
  };
  const quiet = { peak: 0, rms: 0 };
  switch (scenario) {
    case 'main-claude-closed': return { ...ok, claudeWindowOpen: false, claudeListening: false, claude: quiet };
    case 'main-claude-not-listening': return { ...ok, claudeListening: false, claude: quiet };
    case 'main-discord-closed': return { ...ok, discordRunning: false, discordUsesCable: '', discord: quiet };
    case 'main-discord-wrong-mic': return { ...ok, discordUsesCable: 'no' };
    case 'main-discord-restart': return { ...ok, discordUsesCable: 'no', discordNeedsRestart: true };
    case 'error-mic-blocked': return { ...ok, micState: 'blocked', mic: quiet };
    case 'error-mic-missing': return { ...ok, micState: 'missing', micName: '', mic: quiet };
    case 'error-cable-missing': return { ...ok, cableOk: false };
    case 'main-speakers': return { ...ok, speakers: true, echoActive: true };
    case 'main-muted': return { ...ok, mic: quiet };
    case 'onboarding-setup': return { ...ok, engineRunning: t < 1.2 ? false : true };
    default: return ok;
  }
}

const scenarioInfo: AppInfo = {
  version: '1.0.0', windowsBuild: scenario === 'error-unsupported' ? 19045 : 26100,
  supported: scenario !== 'error-unsupported', cableInstalled: scenario !== 'error-cable-missing', testMode: false,
};

let mockConfig = { ...defaultConfig };

// ---- public API -------------------------------------------------------------
export const api = {
  async appInfo(): Promise<AppInfo> {
    return inTauri ? invoke<AppInfo>('app_info') : scenarioInfo;
  },
  async getConfig(): Promise<Config> {
    return inTauri ? invoke<Config>('get_config') : mockConfig;
  },
  async setConfig(patch: Partial<Config>): Promise<Config> {
    if (inTauri) return invoke<Config>('set_config', { patch });
    mockConfig = { ...mockConfig, ...patch };
    return mockConfig;
  },
  async openClaude(): Promise<void> {
    if (inTauri) await invoke('open_claude');
  },
  async openPlace(place: 'discordVoice' | 'micPrivacy' | 'soundSettings' | 'vbAudio' | 'vbAudioDonate' | 'logs' | 'windowsUpdate' | 'download'): Promise<void> {
    if (inTauri) await invoke('open_place', { place });
  },
  async saveReport(): Promise<string> {
    return inTauri ? invoke<string>('save_report') : 'C:\\Users\\dig\\Desktop\\Claude Talk fejlrapport.zip';
  },
  async restoreAudioSettings(): Promise<void> {
    if (inTauri) await invoke('restore_audio_settings');
  },
  async restartDiscord(): Promise<void> {
    if (inTauri) await invoke('restart_discord');
  },
  async selfTest(): Promise<SelfTestResult[]> {
    if (inTauri) return invoke<SelfTestResult[]>('self_test');
    await new Promise((r) => setTimeout(r, 400));
    return [
      { route: 'micToDiscord', passed: true }, { route: 'claudeToDiscord', passed: true },
      { route: 'micToClaude', passed: true }, { route: 'discordToClaude', passed: true },
    ];
  },
  async matrix(): Promise<Matrix> {
    if (inTauri) return invoke<Matrix>('get_matrix');
    const sources = [{ id: 1, label: 'Mikrofon' }, { id: 2, label: 'Claude' }, { id: 3, label: 'Discord' }];
    const outputs = [{ id: 101, label: 'Til Discord' }, { id: 102, label: 'Til Claude' }, { id: 103, label: 'Lyt med' }];
    const on = (s: number, o: number): MatrixCell => ({ source: s, output: o, state: 'on', gainDb: 0 });
    const off = (s: number, o: number): MatrixCell => ({ source: s, output: o, state: 'off', gainDb: 0 });
    const locked = (s: number, o: number, who: string): MatrixCell => ({ source: s, output: o, state: 'locked', gainDb: 0, who });
    return { sources, outputs, cells: [on(1, 101), on(2, 101), locked(3, 101, 'Discord'), on(1, 102), locked(2, 102, 'Claude'), on(3, 102), off(1, 103), off(2, 103), off(3, 103)] };
  },
  async setRoute(source: number, output: number, enabled: boolean, gainDb: number): Promise<void> {
    if (inTauri) await invoke('set_route', { source, output, enabled, gainDb });
  },
  async setUnlocked(unlocked: boolean): Promise<void> {
    if (inTauri) await invoke('set_unlocked', { unlocked });
  },
  onStatus(cb: (s: AudioStatus) => void): () => void {
    if (inTauri) {
      let un: UnlistenFn | undefined;
      let dead = false;
      listen<AudioStatus>('audio-status', (e) => cb(e.payload)).then((u) => (dead ? u() : (un = u)));
      return () => { dead = true; un?.(); };
    }
    const t0 = performance.now();
    const freeze = params.has('freeze');
    const id = setInterval(() => cb(scenarioStatus(freeze ? 2.0 : (performance.now() - t0) / 1000)), 33);
    cb(scenarioStatus(2.0));
    return () => clearInterval(id);
  },
  onConfig(cb: (c: Config) => void): () => void {
    if (!inTauri) return () => {};
    let un: UnlistenFn | undefined;
    let dead = false;
    listen<Config>('config-changed', (e) => cb(e.payload)).then((u) => (dead ? u() : (un = u)));
    return () => { dead = true; un?.(); };
  },
  async windowAction(action: 'minimize' | 'close' | 'toggleMaximize'): Promise<void> {
    if (!inTauri) return;
    const { getCurrentWindow } = await import('@tauri-apps/api/window');
    const w = getCurrentWindow();
    if (action === 'minimize') await w.minimize();
    else if (action === 'toggleMaximize') await w.toggleMaximize();
    else await w.close();
  },
};

export const initialScenario = scenario;
export const isTauri = inTauri;
