// Claude Talk microphone bridge.
//
// Injected into the Claude window before any page script runs. It replaces
// the page's microphone with a live audio track that carries the mix made by
// Claude Talk's audio engine (your microphone + Discord). Nothing else on
// the page is touched, and nothing leaves the page: audio arrives from the
// host application through WebView2 host messages (not the network, not
// subject to the page's CSP), and the only messages sent back are "a
// microphone is in use / not in use" and simple diagnostics.
(() => {
  'use strict';
  if (window.top !== window) return; // audio is delivered to the top document only
  const wv = window.chrome && window.chrome.webview;
  const md = navigator.mediaDevices;
  if (!wv || !md || window.__claudeTalkBridge) return;

  const RATE = 48000;
  const DEVICE_ID = 'claude-talk';
  const GROUP_ID = 'claude-talk';
  const LABEL = 'Claude Talk';
  const post = (msg) => { try { wv.postMessage(JSON.stringify(Object.assign({ ct: 1 }, msg))); } catch (_) { /* host gone */ } };

  const origGetUserMedia = md.getUserMedia ? md.getUserMedia.bind(md) : null;
  const origEnumerate = md.enumerateDevices ? md.enumerateDevices.bind(md) : null;

  // ---- the live source track ------------------------------------------------
  let source = null; // { track, push(Float32Array), mode }
  let liveClones = 0;
  let framesIn = 0;
  let dropped = 0;

  function makeGeneratorSource() {
    // Chromium "breakout box": frames are written straight into the track.
    const gen = new MediaStreamTrackGenerator({ kind: 'audio' });
    const writer = gen.writable.getWriter();
    let ts = 0;
    let pending = 0;
    return {
      mode: 'generator',
      track: gen,
      push(samples) {
        // Never let a stalled consumer build up delay: drop if > 60 ms queued.
        if (pending > 6) { dropped += samples.length; return; }
        const data = new AudioData({ format: 'f32', sampleRate: RATE, numberOfFrames: samples.length, numberOfChannels: 1, timestamp: ts, data: samples });
        ts += Math.round((samples.length * 1e6) / RATE);
        pending++;
        writer.write(data).then(() => { pending--; }, () => { pending--; });
      },
    };
  }

  function makeScriptProcessorSource() {
    // Fallback without modules (works under any CSP): a FIFO read by a
    // ScriptProcessorNode feeding a MediaStreamAudioDestinationNode.
    const ctx = new AudioContext({ sampleRate: RATE, latencyHint: 'interactive' });
    const fifo = new Float32Array(RATE); // 1 s ring
    let w = 0, r = 0, filled = 0;
    const node = ctx.createScriptProcessor(256, 0, 1);
    const dest = ctx.createMediaStreamDestination();
    node.onaudioprocess = (e) => {
      const out = e.outputBuffer.getChannelData(0);
      for (let i = 0; i < out.length; i++) {
        if (filled > 0) { out[i] = fifo[r]; r = (r + 1) % fifo.length; filled--; } else { out[i] = 0; }
      }
      // keep at most ~40 ms queued so latency cannot creep up
      const max = (RATE * 40) / 1000;
      if (filled > max) { const skip = filled - max; r = (r + skip) % fifo.length; filled -= skip; dropped += skip; }
    };
    node.connect(dest);
    const resume = () => { if (ctx.state !== 'running') ctx.resume().catch(() => {}); };
    resume();
    return {
      mode: 'scriptprocessor',
      track: dest.stream.getAudioTracks()[0],
      push(samples) {
        resume();
        for (let i = 0; i < samples.length; i++) {
          fifo[w] = samples[i]; w = (w + 1) % fifo.length;
          if (filled < fifo.length) filled++; else { r = (r + 1) % fifo.length; dropped++; }
        }
      },
    };
  }

  function ensureSource() {
    if (source && source.track.readyState === 'live') return source;
    try {
      source = typeof MediaStreamTrackGenerator === 'function' && typeof AudioData === 'function'
        ? makeGeneratorSource()
        : makeScriptProcessorSource();
    } catch (_) {
      source = makeScriptProcessorSource();
    }
    post({ type: 'source', mode: source.mode });
    return source;
  }

  // ---- make a track look like a normal microphone ----------------------------
  const SETTINGS = {
    deviceId: DEVICE_ID, groupId: GROUP_ID, sampleRate: RATE, sampleSize: 32, channelCount: 1,
    echoCancellation: false, autoGainControl: false, noiseSuppression: false, latency: 0.01,
  };
  function dress(track) {
    try { Object.defineProperty(track, 'label', { value: LABEL, configurable: true }); } catch (_) {}
    track.getSettings = () => Object.assign({}, SETTINGS);
    track.getCapabilities = () => ({
      deviceId: DEVICE_ID, groupId: GROUP_ID, sampleRate: { min: RATE, max: RATE }, sampleSize: { min: 32, max: 32 },
      channelCount: { min: 1, max: 1 }, echoCancellation: [false], autoGainControl: [false], noiseSuppression: [false],
    });
    let constraints = {};
    track.getConstraints = () => Object.assign({}, constraints);
    // Accept any constraints; the signal is already exactly what Claude should hear.
    track.applyConstraints = (c) => { constraints = Object.assign({}, c || {}); return Promise.resolve(); };
    // Every live track and clone counts: the mix is sent while any of them
    // lives (apps clone tracks for recorders and calls, then stop the original).
    const origStop = track.stop.bind(track);
    const origClone = track.clone.bind(track);
    let counted = true;
    liveClones++;
    post({ type: 'mic', active: true });
    const release = () => {
      if (!counted) return;
      counted = false;
      liveClones = Math.max(0, liveClones - 1);
      if (liveClones === 0) post({ type: 'mic', active: false });
    };
    track.stop = () => { release(); origStop(); };
    track.addEventListener('ended', release);
    track.clone = () => dress(origClone());
    return track;
  }

  function newMicTrack() {
    return dress(ensureSource().track.clone());
  }

  // ---- replaced APIs -----------------------------------------------------------
  async function getUserMedia(constraints) {
    if (!constraints || typeof constraints !== 'object' || (!constraints.audio && !constraints.video)) {
      // Same TypeError the real API throws.
      if (origGetUserMedia) return origGetUserMedia(constraints);
      throw new TypeError("Failed to execute 'getUserMedia': audio and/or video is required");
    }
    const tracks = [];
    if (constraints.video) {
      if (!origGetUserMedia) throw new DOMException('Requested device not found', 'NotFoundError');
      const v = await origGetUserMedia({ video: constraints.video });
      tracks.push(...v.getVideoTracks());
    }
    if (constraints.audio) tracks.push(newMicTrack());
    return new MediaStream(tracks);
  }

  async function enumerateDevices() {
    let list = [];
    try { list = origEnumerate ? await origEnumerate() : []; } catch (_) { list = []; }
    const others = list.filter((d) => d.kind !== 'audioinput');
    const mic = (deviceId) => ({ deviceId, kind: 'audioinput', label: LABEL, groupId: GROUP_ID, toJSON() { return { deviceId, kind: 'audioinput', label: LABEL, groupId: GROUP_ID }; } });
    return [mic('default'), mic(DEVICE_ID), ...others];
  }

  const define = (obj, name, value) => { try { Object.defineProperty(obj, name, { value, configurable: true, writable: true }); } catch (_) {} };
  define(md, 'getUserMedia', getUserMedia);
  define(md, 'enumerateDevices', enumerateDevices);
  if (window.MediaDevices && MediaDevices.prototype) {
    define(MediaDevices.prototype, 'getUserMedia', function (c) { return getUserMedia(c); });
    define(MediaDevices.prototype, 'enumerateDevices', function () { return enumerateDevices(); });
  }
  // Legacy callback API
  const legacy = function (c, ok, fail) { getUserMedia(c).then(ok, fail); };
  define(navigator, 'getUserMedia', legacy);
  define(navigator, 'webkitGetUserMedia', legacy);

  if (navigator.permissions && navigator.permissions.query) {
    const origQuery = navigator.permissions.query.bind(navigator.permissions);
    const granted = () => {
      const s = { name: 'microphone', state: 'granted', onchange: null };
      s.addEventListener = () => {}; s.removeEventListener = () => {}; s.dispatchEvent = () => true;
      return s;
    };
    define(navigator.permissions, 'query', (desc) =>
      desc && desc.name === 'microphone' ? Promise.resolve(granted()) : origQuery(desc));
  }

  // ---- audio from the host -------------------------------------------------------
  // Messages are "ct:a:<base64 of little-endian f32 mono samples at 48 kHz>".
  function decode(b64) {
    const bin = atob(b64);
    const bytes = new Uint8Array(bin.length);
    for (let i = 0; i < bin.length; i++) bytes[i] = bin.charCodeAt(i);
    return new Float32Array(bytes.buffer);
  }
  wv.addEventListener('message', (ev) => {
    const d = ev.data;
    if (typeof d !== 'string' || d.charCodeAt(0) !== 99 /* c */ || !d.startsWith('ct:a:')) return;
    if (!source || liveClones === 0) return;
    const samples = decode(d.slice(5));
    framesIn += samples.length;
    source.push(samples);
  });

  setInterval(() => { if (liveClones > 0) post({ type: 'stats', framesIn, dropped, mode: source && source.mode }); }, 2000);
  Object.defineProperty(window, '__claudeTalkBridge', { value: true });
  post({ type: 'ready' });
  // leaving the page (reload, navigation) ends its microphone use
  addEventListener('pagehide', () => { if (liveClones > 0) { liveClones = 0; post({ type: 'mic', active: false }); } });
})();
