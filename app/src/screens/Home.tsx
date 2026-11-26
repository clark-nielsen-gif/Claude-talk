import { useEffect, useRef, useState } from 'react';
import { api, type AppInfo } from '../api';
import { Check, Icon, Meter, Notice, Switch } from '../components';
import { useApp, useT } from '../context';
import { fill } from '../i18n';
import { DiscordGuide } from './Guide';

/** True while the user has spoken within the last 1.5 s. */
function useSpeaking(rms: number): boolean {
  const last = useRef(-Infinity);
  const [speaking, setSpeaking] = useState(false);
  useEffect(() => {
    const now = performance.now();
    if (rms > 0.01) last.current = now; // ≈ -40 dBFS
    setSpeaking(now - last.current < 1500);
  }, [rms]);
  return speaking;
}

export function Home({ info }: { info: AppInfo }) {
  const t = useT();
  const { config, status, update: updateConfig, toast } = useApp();
  const [guide, setGuide] = useState<null | 'mic' | 'restart'>(null);
  const [update, setUpdate] = useState<string | null>(null);
  const [installing, setInstalling] = useState(false);
  const installCable = () => {
    setInstalling(true);
    api.installCable()
      .then((code) => { if (code === 2) toast(t.errors.cableNoInternet); else if (code !== 0) toast(t.errors.cableInstallFailed); })
      .catch((e) => { if (String(e) !== 'cancelled') toast(t.errors.cableInstallFailed); })
      .finally(() => setInstalling(false));
  };
  useEffect(() => api.onUpdate(setUpdate), []);
  const s = status;
  const quiet = { peak: 0, rms: 0 };
  const speaking = useSpeaking(s?.mic.rms ?? 0);
  const on = config.claudeInDiscord;

  const errors = [];
  if (!info.supported) {
    errors.push(<Notice key="u" kind="error" title={t.errors.unsupportedTitle} body={t.errors.unsupportedBody} action={t.errors.unsupportedAction} onAction={() => api.openPlace('windowsUpdate')} />);
  } else if (s && !s.cableOk && !info.cableInstalled) {
    errors.push(<Notice key="c" kind="error" title={t.errors.cableMissingTitle} body={t.errors.cableMissingBody} action={installing ? t.errors.cableInstalling : t.errors.cableMissingAction} onAction={installing ? undefined : installCable} />);
  }
  if (s?.micState === 'blocked') errors.push(<Notice key="b" kind="error" title={t.errors.micBlockedTitle} body={t.errors.micBlockedBody} action={t.errors.micBlockedAction} onAction={() => api.openPlace('micPrivacy')} />);
  if (s?.micState === 'missing') errors.push(<Notice key="m" kind="warn" title={t.errors.micMissingTitle} body={t.errors.micMissingBody} />);
  if (update) errors.push(<Notice key="up" title={t.notices.updateTitle} body={t.notices.updateBody} action={t.notices.updateAction} onAction={() => api.installUpdate().catch(() => toast(t.errors.update))} />);
  if (s?.speakers) errors.push(<Notice key="s" kind={s.echoActive ? 'info' : 'warn'} title={t.notices.speakersTitle} body={s.echoActive ? t.notices.speakersBody : t.notices.speakersBodyOff} />);

  // The cards never contradict the switch or an error above them.
  const cableMissing = !info.supported || (!!s && !s.cableOk && !info.cableInstalled);
  const micBroken = s?.micState === 'blocked' || s?.micState === 'missing';

  // --- Discord card state
  let discordCheck: { state: 'ok' | 'wait' | 'warn'; text: string; fix?: 'mic' | 'restart' };
  if (cableMissing) discordCheck = { state: 'warn', text: t.main.discordNoMic };
  else if (!s?.discordRunning) discordCheck = { state: 'wait', text: t.main.discordNotRunning };
  else if (s.discordNeedsRestart) discordCheck = { state: 'warn', text: t.guide.restartDiscordTitle, fix: 'restart' };
  else if (s.discordUsesCable === 'no') discordCheck = { state: 'warn', text: t.main.discordWrongMic, fix: 'mic' };
  else if (s.discordUsesCable === 'yes') discordCheck = on ? { state: 'ok', text: t.main.discordHearsClaude } : { state: 'wait', text: t.main.claudeOff };
  else discordCheck = { state: 'wait', text: t.main.discordWaiting };

  // --- Claude card state
  const claudeCheck: { state: 'ok' | 'wait' | 'warn'; text: string } = !on
    ? { state: 'wait', text: t.main.claudeOff }
    : !s?.claudeWindowOpen
      ? { state: 'wait', text: t.main.claudeNotOpen }
      : !s.claudeListening
        ? { state: 'wait', text: t.main.claudeOpenNotListening }
        : micBroken
          ? { state: 'warn', text: t.main.micProblem }
          : { state: 'ok', text: t.main.claudeHearsYou };

  return (
    <div className="page">
      <section className="hero" aria-labelledby="switch-title">
        <div className="text">
          <h1 id="switch-title">{t.main.switchTitle}</h1>
          <p className="muted">{on ? t.main.switchOn : t.main.switchOff}</p>
        </div>
        <Switch on={on} label={t.main.switchTitle} onChange={(v) => updateConfig({ claudeInDiscord: v })} />
      </section>

      {errors}

      <div className="people">
        <section className={`person${config.micMuted ? ' dim' : ''}`} aria-label={t.main.you}>
          <div className="head"><h2>{t.main.you}</h2></div>
          <Meter level={config.micMuted ? quiet : s?.mic ?? quiet} off={config.micMuted || s?.micState !== 'ok'} label={t.main.you} />
          <div className="sub" title={s?.micName}>{s?.micName || ' '}</div>
          {micBroken
            ? <Check state="warn">{t.main.micProblem}</Check>
            : config.micMuted
              ? <Check state="warn">{t.main.micMuted}</Check>
              : speaking ? <Check state="ok">{t.main.weHearYou}</Check> : <Check state="wait">{t.main.speakToTest}</Check>}
          <div className="actions">
            {!micBroken && (
              <button className="iconbtn" aria-pressed={config.micMuted} onClick={() => updateConfig({ micMuted: !config.micMuted })}>
                {config.micMuted ? Icon.micOff() : Icon.mic()} {config.micMuted ? t.main.unmuteMe : t.main.muteMe}
              </button>
            )}
          </div>
        </section>

        <section className={`person${!on || !s?.claudeWindowOpen ? ' dim' : ''}`} aria-label={t.main.claude}>
          <div className="head"><h2>{t.main.claude}</h2></div>
          <Meter level={s?.claude ?? quiet} off={!s?.claudeWindowOpen} label={t.main.claude} />
          <div className="sub">&nbsp;</div>
          <Check state={claudeCheck.state}>{claudeCheck.text}</Check>
          <div className="actions">
            {!s?.claudeWindowOpen
              ? <button className="btn primary" onClick={() => api.openClaude()}>{t.main.openClaude}</button>
              : !s.claudeListening && on
                ? <button className="btn secondary" onClick={() => api.openClaude()}>{t.main.showClaude}</button>
                : on && <button className="iconbtn" title={t.main.muteClaudeHint} aria-pressed={config.claudeToDiscordMuted} onClick={() => updateConfig({ claudeToDiscordMuted: !config.claudeToDiscordMuted })}>
                  {config.claudeToDiscordMuted ? Icon.speakerOff() : Icon.speaker()} {config.claudeToDiscordMuted ? t.main.unmuteClaude : t.main.muteClaude}
                </button>}
          </div>
        </section>

        <section className={`person${!s?.discordRunning ? ' dim' : ''}`} aria-label={t.main.discord}>
          <div className="head"><h2>{t.main.discord}</h2></div>
          <Meter level={s?.discord ?? quiet} off={!s?.discordRunning} label={t.main.discord} />
          <div className="sub">&nbsp;</div>
          <Check state={discordCheck.state}>{discordCheck.text}</Check>
          <div className="actions">
            {discordCheck.fix && <button className="btn primary" onClick={() => setGuide(discordCheck.fix!)}>{discordCheck.fix === 'restart' ? t.guide.restartDiscord : t.main.fixIt}</button>}
          </div>
        </section>
      </div>

      {config.pushToTalk && <p className="small">{fill(t.main.pttHint, { key: config.hotkeys.pushToTalkClaude })}</p>}
      {on && s?.discordUsesCable === 'yes' && s.claudeListening && <p className="small">{t.notices.krispTitle} {t.notices.krispBody}</p>}

      {guide && <DiscordGuide mode={guide} onClose={() => setGuide(null)} />}
    </div>
  );
}
