import { useEffect, useRef, useState } from 'react';
import { api, type AppInfo } from '../api';
import { Check, Icon, Meter, Notice, Switch } from '../components';
import { useApp, useT } from '../context';
import { fill } from '../i18n';
import { DiscordGuide } from './Guide';
import { useInstallCable } from '../useInstallCable';

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

export function Home({ info, onSettings }: { info: AppInfo; onSettings: () => void }) {
  const t = useT();
  const { config, status, update: updateConfig, toast } = useApp();
  const [guide, setGuide] = useState<null | 'mic' | 'restart'>(null);
  const [update, setUpdate] = useState<string | null>(null);
  const { installing, install: installCable } = useInstallCable();
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
  if (s?.outputFixed) errors.push(<Notice key="of" kind="warn" title={t.notices.outputFixedTitle} body={t.notices.outputFixedBody} />);
  if (s?.hotkeyProblem) errors.push(<Notice key="hk" kind="warn" title={t.notices.hotkeyTitle} body={s.hotkeyProblem === 'duplicate' ? t.notices.hotkeyDuplicateBody : fill(t.notices.hotkeyBody, { keys: s.hotkeyProblem })} action={t.notices.hotkeyAction} onAction={onSettings} />);
  if (s?.speakers) errors.push(<Notice key="s" kind={s.echoActive ? 'info' : 'warn'} title={t.notices.speakersTitle} body={s.echoActive ? t.notices.speakersBody : t.notices.speakersBodyOff} />);

  // The cards never contradict the switch or an error above them.
  const cableMissing = !info.supported || (!!s && !s.cableOk && !info.cableInstalled);
  const micBroken = s?.micState === 'blocked' || s?.micState === 'missing';

  // --- Discord card state
  let discordCheck: { state: 'ok' | 'wait' | 'warn'; text: string; fix?: 'mic' | 'restart' };
  if (!info.supported) discordCheck = { state: 'warn', text: t.main.discordNeedsWindows11 };
  else if (cableMissing) discordCheck = { state: 'warn', text: t.main.discordNoMic };
  else if (!s?.discordRunning) discordCheck = { state: 'wait', text: t.main.discordNotRunning };
  else if (s.discordNeedsRestart) discordCheck = { state: 'warn', text: t.guide.restartDiscordTitle, fix: 'restart' };
  else if (s.discordUsesCable === 'no') discordCheck = { state: 'warn', text: t.main.discordWrongMic, fix: 'mic' };
  else if (s.discordUsesCable === 'yes') {
    discordCheck = !on
      ? { state: 'wait', text: t.main.discordHearsOnlyYou }
      : config.claudeToDiscordMuted
        ? { state: 'wait', text: t.main.claudeMutedInDiscord }
        : !s.claudeWindowOpen || !s.claudeListening
          ? { state: 'wait', text: t.main.discordReadyForClaude }
          : { state: 'ok', text: t.main.discordHearsClaude };
  }
  else discordCheck = { state: 'wait', text: t.main.discordWaiting };

  // --- Claude card state: built from what Claude really hears right now
  const hearsYou = !micBroken && !config.micMuted;
  const hearsDiscord = !!s?.discordRunning && config.claudeHearsDiscord;
  const claudeCheck: { state: 'ok' | 'wait' | 'warn'; text: string } = !on
    ? { state: 'wait', text: t.main.claudeOff }
    : cableMissing
      ? { state: 'wait', text: t.main.claudeNotReady }
      : !s?.claudeWindowOpen
        ? { state: 'wait', text: t.main.claudeNotOpen }
        : !s.claudeListening
          ? { state: 'wait', text: t.main.claudeOpenNotListening }
          : micBroken
            ? { state: 'warn', text: t.main.claudeCantHearYou }
            : hearsYou && config.pushToTalk
              ? { state: 'ok', text: fill(t.main.claudeHearsYouPtt, { key: config.hotkeys.pushToTalkClaude }) }
            : hearsYou && hearsDiscord
              ? { state: 'ok', text: t.main.claudeHearsYou }
              : hearsYou
                ? { state: 'ok', text: t.main.claudeHearsOnlyYou }
                : hearsDiscord
                  ? { state: 'ok', text: t.main.claudeHearsOnlyDiscord }
                  : { state: 'wait', text: t.main.claudeHearsNobody };
  // The line under the switch says the same as the cards and notices below.
  const discordBroken = discordCheck.state === 'warn';
  const switchText = !on ? t.main.switchOff
    : cableMissing ? t.main.switchOnNotReady
    : micBroken ? t.main.switchOnMicProblem
    : discordBroken ? t.main.switchOnFixDiscord
    : !s?.discordRunning ? t.main.switchOnNoDiscord
    : !s.claudeWindowOpen ? t.main.switchOnClaudeClosed
    : !s.claudeListening ? t.main.switchOnClaudeNotListening
    : config.claudeToDiscordMuted ? t.main.switchOnClaudeMuted
    : config.micMuted ? t.main.switchOnMicMuted
    : !config.claudeHearsDiscord ? t.main.switchOnOnlyYou
    : t.main.switchOn;
  const switchWarn = on && (cableMissing || micBroken || discordBroken);

  return (
    <div className="page">
      <section className="hero" aria-labelledby="switch-title">
        <div className="text">
          <h1 id="switch-title">{t.main.switchTitle}</h1>
          <p className={switchWarn ? 'warn-text' : 'muted'}>{switchText}</p>
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
          <Meter level={on ? s?.claude ?? quiet : quiet} off={!on || !s?.claudeWindowOpen} label={t.main.claude} />
          <div className="sub">&nbsp;</div>
          <Check state={claudeCheck.state}>{claudeCheck.text}</Check>
          <div className="actions">
            {!s?.claudeWindowOpen
              ? <button className="btn primary" onClick={() => api.openClaude()}>{t.main.openClaude}</button>
              : !s.claudeListening && on
                ? <button className="btn secondary" onClick={() => api.openClaude()}>{t.main.showClaude}</button>
                : on && !cableMissing && <button className="iconbtn" title={t.main.muteClaudeHint} aria-pressed={config.claudeToDiscordMuted} onClick={() => updateConfig({ claudeToDiscordMuted: !config.claudeToDiscordMuted })}>
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

      {guide && <DiscordGuide mode={guide} onClose={() => setGuide(null)} />}
    </div>
  );
}
