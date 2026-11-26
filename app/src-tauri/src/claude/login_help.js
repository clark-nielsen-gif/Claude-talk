// Shown inside the Claude window when "Continue with Google" is pressed:
// Google does not allow signing in from apps' built-in browser windows, so
// we explain the e-mail sign-in instead. Pure DOM, no network, no storage.
(() => {
  if (window.__claudeTalkLoginHelp) return;
  const HOW_DA = 'Skriv din e-mail i feltet på Claudes side, og tryk på knappen under det (den hedder »Continue with email«). Claude sender dig en mail med en kode. Skriv koden her i Claude-vinduet. Tryk ikke på linket i mailen: så logger du ind i din almindelige browser i stedet for her. Har du trykket på linket alligevel? Så kom tilbage hertil, og skriv koden fra mailen her. Du skal kun logge ind én gang.';
  const HOW_EN = "Type your e-mail address in the field on Claude's page and press the button below it (“Continue with email”). Claude e-mails you a code. Type the code here in the Claude window. Don't click the link in the e-mail: that signs you in in your normal browser instead of here. Did you click the link anyway? Then come back here and type the code from the e-mail. You only sign in once.";
  const TEXT = {
    da: {
      start: { title: 'Log ind på Claude med din e-mail', body: HOW_DA },
      google: { title: 'Log ind med din e-mail', body: 'Google tillader ikke login i programmers egne vinduer. ' + HOW_DA },
      ok: 'Forstået',
    },
    en: {
      start: { title: 'Sign in to Claude with your e-mail', body: HOW_EN },
      google: { title: 'Sign in with your e-mail', body: "Google doesn't allow signing in inside apps' own windows. " + HOW_EN },
      ok: 'Got it',
    },
  };
  window.__claudeTalkLoginHelp = (lang, mode) => {
    const all = TEXT[lang] || TEXT.da;
    const t = { ...(all[mode] || all.google), ok: all.ok };
    const old = document.getElementById('claude-talk-login-help');
    if (old) old.remove();
    const host = document.createElement('div');
    host.id = 'claude-talk-login-help';
    const root = host.attachShadow({ mode: 'closed' });
    root.innerHTML = `
      <style>
        .back { position: fixed; inset: 0; background: rgba(31,30,29,.38); display: flex; align-items: center; justify-content: center; z-index: 2147483647;
                font: 15px/1.55 system-ui, "Segoe UI", sans-serif; animation: f .18s ease-out; }
        .card { background: #FAF9F5; color: #1F1E1D; max-width: 440px; margin: 24px; padding: 28px 28px 22px; border-radius: 16px;
                border: 1px solid #E8E6DC; box-shadow: 0 18px 50px rgba(31,30,29,.22); }
        h2 { margin: 0 0 10px; font: 600 22px/1.25 Georgia, "Source Serif 4", serif; letter-spacing: -.01em; }
        p { margin: 0 0 20px; color: #3D3D3A; }
        button { background: #B4532F; color: #fff; border: 0; border-radius: 10px; padding: 10px 18px; font: 600 15px system-ui, "Segoe UI", sans-serif; cursor: pointer; }
        button:hover { background: #9E4626; } button:focus-visible { outline: 3px solid #1F1E1D; outline-offset: 2px; }
        @keyframes f { from { opacity: 0 } to { opacity: 1 } }
        @media (prefers-reduced-motion: reduce) { .back { animation: none } }
        @media (prefers-color-scheme: dark) { .card { background: #262624; color: #F5F4EE; border-color: #3A3936 } p { color: #C9C7BE } }
      </style>
      <div class="back" role="dialog" aria-modal="true" aria-labelledby="t">
        <div class="card"><h2 id="t"></h2><p></p><button type="button"></button></div>
      </div>`;
    root.querySelector('h2').textContent = t.title;
    root.querySelector('p').textContent = t.body;
    const b = root.querySelector('button');
    b.textContent = t.ok;
    b.addEventListener('click', () => host.remove());
    document.documentElement.appendChild(host);
    b.focus();
  };
})();
