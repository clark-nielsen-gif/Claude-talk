import { Fragment, useEffect, useRef, useState } from 'react';
import { api, type AppInfo } from '../api';
import { Check, Icon, Meter, Notice, Switch } from '../components';
import { useApp, useT } from '../context';
import { fill } from '../i18n';
import { DiscordGuide } from './Guide';
import { useInstallCable } from '../useInstallCable';
import { useCableBroken } from '../useCableBroken';

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

export function Home({ info, onSettings, onHotkeys, onEcho }: { info: AppInfo; onSettings: () => void; onHotkeys: () => void; onEcho: () => void }) {
  const t = useT();
  const { config, status, update: updateConfig, toast } = useApp();
  const [guide, setGuide] = useState<null | 'mic' | 'restart'>(null);
  const [update, setUpdate] = useState<string | null>(null);
  const { installing, install: installCable } = useInstallCable();
  // "Åbn Claude" waits for the window (seconds on a first start): one press
  const [opening, setOpening] = useState(false);
  const openClaude = () => {
    if (opening) return;
    setOpening(true);
    api.openClaude().catch(() => toast(t.errors.claudeWindow)).finally(() => setTimeout(() => setOpening(false), 1500));
  };
  // The cable stopped working while installed: shown after 5 s, not for a blip.
  const cableBroken = useCableBroken(status, info);
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
  if (cableBroken) errors.push(<Notice key="cb" kind="error" title={t.errors.cableBrokenTitle} body={t.errors.cableBrokenBody} action={t.errors.cableBrokenAction} onAction={() => api.openPlace('soundSettings')} />);
  if (s?.micState === 'blocked') errors.push(<Notice key="b" kind="error" title={t.errors.micBlockedTitle} body={t.errors.micBlockedBody} action={t.errors.micBlockedAction} onAction={() => api.openPlace('micPrivacy')} />);
  if (s?.micState === 'missing') errors.push(<Notice key="m" kind="warn" title={t.errors.micMissingTitle} body={t.errors.micMissingBody} />);
  if (update) errors.push(<Notice key="up" title={t.notices.updateTitle} body={t.notices.updateBody} action={t.notices.updateAction} onAction={() => api.installUpdate().catch(() => toast(t.errors.update))} />);
  if (s?.outputFixed) errors.push(<Notice key="of" kind="warn" title={t.notices.outputFixedTitle} body={t.notices.outputFixedBody} />);
  if (s?.hotkeyProblem) errors.push(<Notice key="hk" kind="warn" title={t.notices.hotkeyTitle} body={s.hotkeyProblem === 'duplicate' ? t.notices.hotkeyDuplicateBody : fill(t.notices.hotkeyBody, { keys: s.hotkeyProblem })} action={t.notices.hotkeyAction} onAction={onHotkeys} />);
  if (s?.speakers) errors.push(<Notice key="s" kind={s.echoActive ? 'info' : 'warn'} title={t.notices.speakersTitle} body={s.echoActive ? t.notices.speakersBody : t.notices.speakersBodyOff} action={s.echoActive ? undefined : t.notices.echoAction} onAction={s.echoActive ? undefined : onEcho} />);

  // The cards never contradict the switch or an error above them.
  const cableMissing = !info.supported || (!!s && !s.cableOk && !info.cableInstalled) || cableBroken;
  const micBroken = s?.micState === 'blocked' || s?.micState === 'missing';

  // --- Discord card state
  // Claude in Chrome, Edge or the Claude app (Avanceret) is the Claude in the
  // call instead of the Claude window
  const external = !!config.externalClaude;
  const claudeIn = external || (!!s?.claudeWindowOpen && !s?.claudeLogin && !!s?.claudeListening);
  let discordCheck: { state: 'ok' | 'wait' | 'warn'; text: string; fix?: 'mic' | 'restart' };
  if (!info.supported) discordCheck = { state: 'warn', text: t.main.discordNeedsWindows11 };
  else if (cableBroken) discordCheck = { state: 'warn', text: t.main.discordCableBroken };
  else if (cableMissing) discordCheck = { state: 'warn', text: t.main.discordNoMic };
  else if (!s?.discordRunning) discordCheck = { state: 'wait', text: t.main.discordNotRunning };
  else if (s.discordNeedsRestart) discordCheck = { state: 'warn', text: t.guide.restartDiscordTitle, fix: 'restart' };
  else if (s.discordUsesCable === 'no') {
    // after "Gendan normale lydindstillinger" Discord is meant to use the
    // user's own microphone until Claude is switched on again
    discordCheck = config.routeDiscord
      ? { state: 'warn', text: t.main.discordWrongMic, fix: 'mic' }
      : { state: 'wait', text: t.main.discordOwnMic };
  }
  else if (s.discordUsesCable === 'yes') {
    discordCheck = micBroken
      ? { state: 'warn', text: t.main.discordCantHearYou }
      : !on
      ? { state: 'wait', text: t.main.discordHearsOnlyYou }
      : config.claudeToDiscordMuted
        ? { state: 'wait', text: t.main.claudeMutedInDiscord }
        : !claudeIn
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
      : external
        ? (micBroken ? { state: 'warn', text: t.main.claudeCantHearYou } : { state: 'ok', text: t.main.claudeExternal })
      : !s?.claudeWindowOpen
        ? { state: 'wait', text: t.main.claudeNotOpen }
        : s.claudeLogin
          ? { state: 'wait', text: t.main.claudeLogIn }
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
  // Claude waits for the user (log in, open, press the voice button)
  const claudeNeedsYou = on && !cableMissing && !external && (!s?.claudeWindowOpen || !!s?.claudeLogin || !s?.claudeListening);
  // The line under the switch says the same as the cards and notices below.
  const discordBroken = discordCheck.state === 'warn';
  const switchText = !on
    ? (cableMissing || micBroken ? t.main.switchOffProblem
      : config.micMuted && config.routeDiscord ? t.main.switchOffMuted : t.main.switchOff)
    : cableMissing ? t.main.switchOnNotReady
    : micBroken ? t.main.switchOnMicProblem
    : discordBroken ? t.main.switchOnFixDiscord
    : !s?.discordRunning ? t.main.switchOnNoDiscord
    : !external && !s.claudeWindowOpen ? t.main.switchOnClaudeClosed
    : !external && s.claudeLogin ? t.main.switchOnClaudeLogin
    : !external && !s.claudeListening ? t.main.switchOnClaudeNotListening
    : s.discordUsesCable !== 'yes' ? t.main.switchOnNoCall
    : config.claudeToDiscordMuted ? t.main.switchOnClaudeMuted
    : config.micMuted ? t.main.switchOnMicMuted
    : config.pushToTalk ? fill(t.main.switchOnPtt, { key: config.hotkeys.pushToTalkClaude })
    : !config.claudeHearsDiscord ? t.main.switchOnOnlyYou
    : t.main.switchOn;
  const switchWarn = cableMissing || micBroken || (on && discordBroken);

  return (
    <div className="page">
      <section className="hero" aria-labelledby="switch-title">
        <div className="text">
          <h1 id="switch-title">{t.main.switchTitle}</h1>
          <p className={switchWarn ? 'warn-text' : 'muted'} role="status">{switchText}</p>
        </div>
        <Switch on={on} label={t.main.switchTitle} onChange={(v) => updateConfig({ claudeInDiscord: v })} />
      </section>

      {errors}

      <div className="people">
        {[
          { key: 'you', first: micBroken, card: (
        <section className={`person${config.micMuted ? ' dim' : ''}${micBroken ? ' attention' : ''}`} aria-label={t.main.you}>
          <div className="head"><h2>{t.main.you}</h2></div>
          <Meter level={config.micMuted ? quiet : s?.mic ?? quiet} off={config.micMuted || s?.micState !== 'ok'} label={t.main.you} />
          <div className="sub" title={s?.micName}>{s?.micName || ' '}</div>
          {micBroken
            ? <Check state="warn">{t.main.micProblem}</Check>
            : config.micMuted
              ? <Check state="warn">{!config.routeDiscord || s?.discordUsesCable === 'no' ? t.main.micMutedClaudeOnly : t.main.micMuted}</Check>
              : speaking ? <Check state="ok">{t.main.weHearYou}</Check> : <Check state="wait">{t.main.speakToTest}</Check>}
          <div className="actions">
            {!micBroken && (
              <button className="iconbtn" onClick={() => updateConfig({ micMuted: !config.micMuted })}>
                {config.micMuted ? Icon.micOff() : Icon.mic()} {config.micMuted ? t.main.unmuteMe : t.main.muteMe}
              </button>
            )}
            {s?.micState !== 'blocked' && (
              <button className="btn quiet" onClick={onSettings}>{t.main.otherMic}</button>
            )}
          </div>
        </section>
          ) },
          { key: 'claude', first: claudeCheck.state === 'warn' || claudeNeedsYou, card: (

        <section className={`person${!on || !s?.claudeWindowOpen ? ' dim' : ''}${claudeCheck.state === 'warn' ? ' attention' : ''}`} aria-label={t.main.claude}>
          <div className="head"><h2>{t.main.claude}</h2></div>
          <Meter level={on ? s?.claude ?? quiet : quiet} off={!on || !claudeIn} label={t.main.claude} />
          <div className="sub blank">&nbsp;</div>
          <Check state={claudeCheck.state}>{claudeCheck.text}</Check>
          <div className="actions">
            {!external && !s?.claudeWindowOpen
              ? <button className="btn primary" disabled={opening} onClick={openClaude}>{opening ? t.main.openingClaude : t.main.openClaude}</button>
              : !external && !s?.claudeListening && on
                ? <button className="btn secondary" onClick={openClaude}>{t.main.showClaude}</button>
                : on && !cableMissing && <button className="iconbtn" aria-describedby="mute-claude-hint" title={t.main.muteClaudeHint} onClick={() => updateConfig({ claudeToDiscordMuted: !config.claudeToDiscordMuted })}>
                  {config.claudeToDiscordMuted ? Icon.speakerOff() : Icon.speaker()} {config.claudeToDiscordMuted ? t.main.unmuteClaude : t.main.muteClaude}
                </button>}
          </div>
        </section>
          ) },
          { key: 'discord', first: discordCheck.state === 'warn', card: (

        <section className={`person${!s?.discordRunning ? ' dim' : ''}${discordCheck.state === 'warn' ? ' attention' : ''}`} aria-label={t.main.discord}>
          <div className="head"><h2>{t.main.discord}</h2></div>
          <Meter level={s?.discord ?? quiet} off={!s?.discordRunning} label={t.main.discord} />
          <div className="sub blank">&nbsp;</div>
          <Check state={discordCheck.state}>{discordCheck.text}</Check>
          <div className="actions">
            {discordCheck.fix && <button className="btn primary" onClick={() => setGuide(discordCheck.fix!)}>{discordCheck.fix === 'restart' ? t.guide.restartDiscord : t.main.fixIt}</button>}
          </div>
        </section>
          ) },
          // the card that needs something from the user comes first, also
          // for the keyboard and screen readers (not only on screen)
        ].sort((a, b) => Number(b.first) - Number(a.first)).map((c) => <Fragment key={c.key}>{c.card}</Fragment>)}
      </div>

      {on && <p className="small">{t.notices.discordTips}</p>}
      <span id="mute-claude-hint" className="sr-only">{t.main.muteClaudeHint}</span>

      {guide && <DiscordGuide mode={guide} onClose={() => setGuide(null)} onSettings={() => { setGuide(null); onSettings(); }} />}
    </div>
  );
}
