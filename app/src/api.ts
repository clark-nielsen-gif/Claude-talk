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
  /** Discord's microphone is pointed at Claude Talk (off after "Gendan"). */
  routeDiscord: boolean;
  /** Chosen microphone (endpoint id); null = Windows' default. */
  microphone: string | null;
  microphoneName?: string | null;
  micMuted: boolean;
  claudeToDiscordMuted: boolean;
  pushToTalk: boolean;
  echo: EchoMode;
  hotkeys: Hotkeys;
  advancedUnlocked: boolean;
  presets?: { name: string }[];
  externalClaude?: string | null;
  instructions: Instructions;
  claudeHearsDiscord: boolean;
  closeHintShown: boolean;
}

export interface Instructions {
  onlyWhenCalled: boolean;
  wakeWord: string;
  short: boolean;
  danish: boolean;
  group: boolean;
  own: string;
  autoSend: boolean;
}

export interface Choice { value: string; label: string }
export interface MixerChoices { inputs: Choice[]; programs: Choice[]; outputs: Choice[] }

export interface Level { peak: number; rms: number }

export interface AudioStatus {
  engineRunning: boolean;
  mic: Level; claude: Level; discord: Level; toDiscord: Level; toClaude: Level;
  micState: 'ok' | 'missing' | 'blocked' | 'starting' | '';
  micName: string;
  claudeWindowOpen: boolean;
  claudeListening: boolean;
  /** the Claude window shows claude.ai's login page */
  claudeLogin: boolean;
  discordRunning: boolean;
  /** 'yes' Discord records our cable, 'no' it records another mic, '' unknown */
  discordUsesCable: 'yes' | 'no' | '';
  discordNeedsRestart: boolean;
  /** Discord itself is set to "Claude Talk – til Discord" (seen after Gendan) */
  discordCableChosen?: boolean;
  /** Discord still records the cable it opened before Gendan (K12) */
  discordStillOnCable?: boolean;
  /** Windows' default output was our cable and was just set back */
  outputFixed: boolean;
  micFixed?: boolean;
  /** the device the output or microphone was set to */
  fixedDevice?: string | null;
  /** shortcuts that could not be registered: '' fine, 'duplicate', or the keys */
  hotkeyProblem: string;
  /** Discord plays on another device than Claude: [Discord's, Claude's]. */
  outputMismatch?: [string, string] | null;
  claudeExternal?: Level;
  claudeExternalRunning?: boolean;
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
  /** An extra virtual cable the user installed ("Claude in Chrome or the app"). */
  extraCable?: { name: string; output: string | null } | null;
  testMode: boolean;
}

export interface Route { source: number; output: number; gainDb: number; muted: boolean }
export interface MatrixCell { source: number; output: number; state: 'on' | 'off' | 'locked' | 'loop'; gainDb: number; who?: string }
export interface Labeled { id: number; label: string; removable: boolean }
export interface Matrix { sources: Labeled[]; outputs: Labeled[]; cells: MatrixCell[] }
export interface SelfTestResult { route: string; passed: boolean }

const inTauri = typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window;

// ---- scenario backend (screenshots only) ----------------------------------
const params = new URLSearchParams(typeof location !== 'undefined' ? location.search : '');
const scenario = params.get('scenario') ?? 'main-ok';

const defaultConfig: Config = {
  version: 1, language: (params.get('lang') as 'da' | 'en') ?? 'da', theme: (params.get('theme') as Theme) ?? 'light',
  autostart: true, onboardingDone: !scenario.startsWith('onboarding'), claudeInDiscord: scenario !== 'main-off' && !scenario.startsWith('main-restored'), routeDiscord: !scenario.startsWith('main-restored'), microphone: null,
  micMuted: scenario === 'main-muted', claudeToDiscordMuted: false, pushToTalk: false, echo: 'auto',
  hotkeys: { muteClaudeToDiscord: 'Ctrl+Shift+F9', muteMic: 'Ctrl+Shift+F10', pushToTalkClaude: 'Ctrl+Shift+F11' },
  advancedUnlocked: false,
  presets: [{ name: 'Musik' }],
  instructions: { onlyWhenCalled: scenario === 'instructions', wakeWord: 'Claude', short: scenario === 'instructions', danish: true, group: true, own: '', autoSend: true },
  claudeHearsDiscord: true, closeHintShown: true,
  externalClaude: scenario === 'advanced-extra-cable' ? 'chrome.exe' : null,
};

function scenarioStatus(t: number): AudioStatus {
  const wave = (f: number, a: number) => Math.max(0, a * (0.55 + 0.45 * Math.sin(t * f)));
  const lv = (a: number, f = 7) => ({ peak: wave(f, a), rms: wave(f, a) * 0.6 });
  const ok: AudioStatus = {
    engineRunning: true, mic: lv(0.5), claude: lv(0.6, 5), discord: lv(0.35, 9), toDiscord: lv(0.6, 6), toClaude: lv(0.45, 8),
    micState: 'ok', micName: 'Headset-mikrofon (Jabra Evolve2)', claudeWindowOpen: true, claudeListening: true, claudeLogin: false,
    discordRunning: true, discordUsesCable: 'yes', discordNeedsRestart: false, outputFixed: false, hotkeyProblem: '', speakers: false, echoActive: false, cableOk: true,
    latencyMs: 31, driftPpm: -12, xruns: 0,
  };
  const quiet = { peak: 0, rms: 0 };
  switch (scenario) {
    case 'main-claude-closed': return { ...ok, claudeWindowOpen: false, claudeListening: false, claude: quiet };
    case 'main-claude-login': return { ...ok, claudeListening: false, claudeLogin: true, claude: quiet };
    case 'main-claude-not-listening': return { ...ok, claudeListening: false, claude: quiet };
    case 'main-discord-closed': return { ...ok, discordRunning: false, discordUsesCable: '', discord: quiet };
    case 'main-discord-wrong-mic': return { ...ok, discordUsesCable: 'no' };
    case 'main-discord-restart': return { ...ok, discordUsesCable: 'no', discordNeedsRestart: true };
    case 'error-mic-blocked': return { ...ok, micState: 'blocked', mic: quiet };
    case 'error-mic-missing': return { ...ok, micState: 'missing', micName: '', mic: quiet };
    case 'error-cable-missing': return { ...ok, cableOk: false };
    case 'main-speakers': return { ...ok, speakers: true, echoActive: true, micName: 'Mikrofon (Realtek High Definition Audio)' };
    case 'main-muted': return { ...ok, mic: quiet };
    case 'main-output-elsewhere': return { ...ok, outputMismatch: ['Headset (Jabra Evolve2)', 'Højttalere (Realtek High Definition Audio)'] };
    case 'main-restored': return { ...ok, discordUsesCable: 'no', claudeListening: false };
    case 'main-restored-still-on-cable': return { ...ok, discordUsesCable: 'yes', discordStillOnCable: true, claudeListening: false };
    case 'main-restored-cable-chosen': return { ...ok, discordUsesCable: 'yes', discordCableChosen: true, claudeListening: false };
    case 'main-output-fixed': return { ...ok, outputFixed: true, fixedDevice: 'Højttalere (Realtek(R) Audio)' };
    case 'main-hotkey-taken': return { ...ok, hotkeyProblem: 'Ctrl+Shift+F9' };
    case 'onboarding-cable-missing': return { ...ok, cableOk: false };
    case 'onboarding-mic-blocked': return { ...ok, micState: 'blocked', mic: quiet };
    case 'onboarding-setup': return { ...ok, engineRunning: t < 1.2 ? false : true };
    default: return ok;
  }
}

const scenarioInfo: AppInfo = {
  version: '1.0.0', windowsBuild: scenario === 'error-unsupported' ? 19045 : 26100,
  supported: scenario !== 'error-unsupported', cableInstalled: scenario !== 'error-cable-missing' && scenario !== 'onboarding-cable-missing', testMode: false,
  extraCable: scenario === 'advanced-extra-cable' ? { name: 'CABLE-A Input (VB-Audio Cable A)', output: 'CABLE-A Output (VB-Audio Cable A)' } : null,
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
  async openPlace(place: 'discordVoice' | 'micPrivacy' | 'soundSettings' | 'vbAudio' | 'vbAudioDonate' | 'logs' | 'windowsUpdate' | 'download' | 'support'): Promise<void> {
    if (inTauri) await invoke('open_place', { place });
  },
  async saveReport(): Promise<string> {
    return inTauri ? invoke<string>('save_report') : 'C:\\Users\\dig\\Desktop\\Claude Talk fejlrapport.zip';
  },
  async useDiscordOutput(): Promise<void> {
    if (inTauri) await invoke('use_discord_output');
  },
  async listMicrophones(): Promise<{ id: string; name: string }[]> {
    return inTauri ? invoke('list_microphones') : [{ id: 'jabra', name: 'Headset-mikrofon (Jabra Evolve2)' }, { id: 'realtek', name: 'Mikrofon (Realtek High Definition Audio)' }];
  },
  async setMicrophone(id: string | null): Promise<Config> {
    if (inTauri) return invoke<Config>('set_microphone', { id });
    mockConfig = { ...mockConfig, microphone: id };
    return mockConfig;
  },
  async restoreAudioSettings(): Promise<void> {
    if (inTauri) await invoke('restore_audio_settings');
  },
  /** Installs Discord's microphone (one Windows prompt). Resolves to the
   *  installer's exit code; rejects with 'cancelled' if the prompt is declined. */
  async installCable(): Promise<number> {
    return inTauri ? invoke<number>('install_cable') : 0;
  },
  /** The exact message the instructions become. */
  async instructionText(instructions: Instructions, language: string, hearsOthers: boolean): Promise<string> {
    if (inTauri) return invoke<string>('instruction_text', { instructions, language, hearsOthers });
    return 'Du er med i et Discord-opkald med flere personer. Du hører alle i opkaldet, og de hører dig. Svar kun, når nogen siger »Claude«. Hvis ingen siger »Claude«, så sig ingenting og vent. Hold dine svar korte: en til tre sætninger. Du behøver ikke svare på denne besked – bare følg den.';
  },
  /** Writes the instructions into the Claude window and sends them. */
  async sendInstructions(): Promise<void> {
    if (inTauri) await invoke('send_instructions');
  },
  /** Whether the Claude window accepted the instructions. */
  onPrompt(cb: (r: { ok: boolean; reason: string }) => void): () => void {
    if (!inTauri) return () => {};
    let un: UnlistenFn | undefined;
    let dead = false;
    listen<{ ok: boolean; reason: string }>('claude-prompt', (e) => cb(e.payload)).then((u) => (dead ? u() : (un = u)));
    return () => { dead = true; un?.(); };
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
    const L = (id: number, label: string, removable = false): Labeled => ({ id, label, removable });
    const sources = [L(1, 'Mikrofon'), L(2, 'Claude'), L(3, 'Discord'), L(200, 'Spotify', true)];
    const outputs = [L(101, 'Til Discord'), L(102, 'Til Claude'), L(300, 'Headset', true)];
    const on = (s: number, o: number, gainDb = 0): MatrixCell => ({ source: s, output: o, state: 'on', gainDb });
    const off = (s: number, o: number): MatrixCell => ({ source: s, output: o, state: 'off', gainDb: 0 });
    const locked = (s: number, o: number, who: string): MatrixCell => ({ source: s, output: o, state: 'locked', gainDb: 0, who });
    return {
      sources, outputs,
      cells: [on(1, 101), on(2, 101), locked(3, 101, 'Discord'), on(1, 102), locked(2, 102, 'Claude'), on(3, 102),
        locked(1, 300, 'Du'), off(2, 300), off(3, 300), on(200, 101, -12), off(200, 102), off(200, 300)],
    };
  },
  async mixerChoices(): Promise<MixerChoices> {
    if (inTauri) return invoke<MixerChoices>('mixer_choices');
    return {
      inputs: [{ value: 'in1', label: 'Mikrofon (USB Audio)' }, { value: 'in2', label: 'Line In (Realtek)' }],
      programs: [{ value: 'spotify.exe', label: 'Spotify' }, { value: 'vlc.exe', label: 'Vlc' }],
      outputs: [{ value: 'out1', label: 'Headset (Jabra Evolve2)' }, { value: 'out2', label: 'Højttalere (Realtek)' }],
    };
  },
  async addMixerSource(kind: 'input' | 'program', value: string, label: string): Promise<number> {
    return inTauri ? invoke<number>('add_mixer_source', { kind, value, label }) : 201;
  },
  async removeMixerSource(id: number): Promise<void> {
    if (inTauri) await invoke('remove_mixer_source', { id });
  },
  async addMixerOutput(deviceId: string, label: string): Promise<number> {
    return inTauri ? invoke<number>('add_mixer_output', { deviceId, label }) : 301;
  },
  async removeMixerOutput(id: number): Promise<void> {
    if (inTauri) await invoke('remove_mixer_output', { id });
  },
  async savePreset(name: string): Promise<void> {
    if (inTauri) await invoke('save_preset', { name });
  },
  async loadPreset(name: string): Promise<void> {
    if (inTauri) await invoke('load_preset', { name });
  },
  async setExternalClaude(exe: string | null): Promise<void> {
    if (inTauri) await invoke('set_external_claude', { exe });
    else mockConfig = { ...mockConfig, externalClaude: exe };
  },
  async deletePreset(name: string): Promise<void> {
    if (inTauri) await invoke('delete_preset', { name });
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
  onUpdate(cb: (version: string) => void): () => void {
    if (!inTauri) { if (scenario === 'main-update') cb('1.1.0'); return () => {}; }
    let un: UnlistenFn | undefined;
    let dead = false;
    listen<string>('update-available', (e) => cb(e.payload)).then((u) => (dead ? u() : (un = u)));
    return () => { dead = true; un?.(); };
  },
  async installUpdate(): Promise<void> {
    if (inTauri) await invoke('install_update');
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
