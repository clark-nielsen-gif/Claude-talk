# Claude Talk – plan (arkitektur 2: én virtuel mikrofon)

Repositoryet er privat. Undervejs kører hurtige tests kun på `windows-latest`. Matrixen (`windows-latest`, `windows-2025`, `windows-2022`) køres ved milepæle, og 2-timers udholdenhedstesten køres én gang til sidst. Rene DSP-tests kører også på Linux i udviklingsmiljøet.

```
 Fysisk mikrofon ─┐
                  ├─► [Til Discord] ─► VB-CABLE "Claude Talk – til Discord (VB-Audio)" ─► Discord (mikrofon)
 Claude-vinduet ──┤      (Mikrofon + Claude)
 (process loopback│
  af WebView2-    │
  processtræet)   │
                  │
 Discord ─────────┼─► [Til Claude] ─► host-besked ─► injiceret getUserMedia i Claude-vinduet
 (process loopback)      (Mikrofon + Discord)
```
Brugeren hører Claude og Discord helt normalt (ingen omdirigering af afspilning).

## M0 – Fundament ✅
VB-CABLE: Microsoft-attesteret, lydløs installation, omdøbt og verificeret i CI.

## M1 – Lydmotor (Rust, `engine/`)
- **M1a:** låsefri SPSC-ringbuffer, mixer/routing-graf med løkkedetektion og hårde låse, gain/mute, true-peak limiter, adaptiv resampler med drift-regulering, niveaumålere. Unit tests og `proptest`.
- **M1b:** WASAPI event-drevet capture/render, process loopback (include tree), MMCSS "Pro Audio", genfinding af PID, enhedsskift, sleep/resume, logning uden for lydtråden.

## M2 – Claude-vinduet
- Separat WebView2-miljø (egen brugerdatamappe og dermed eget processtræ, som giver isoleret process loopback af Claudes stemme).
- Injiceret script (`AddScriptToExecuteOnDocumentCreated`), der erstatter `getUserMedia`, `enumerateDevices` og `permissions.query`. Lyd fra motoren via WebView2-hostbeskeder.
- `PermissionRequested`: mikrofon kun for claude.ai. Navigation uden for claude.ai og Anthropics login åbnes i standardbrowseren. Venlig guide, hvis Google-login blokeres.
- Lokal testside med streng CSP, der efterligner en stemme-webapp. Den kan kun aktiveres i CI-builds.

## M3 – End-to-end-tests i CI
Testsiden 440 Hz, falsk Discord 1000 Hz, falsk mikrofon 2500 Hz, baggrund 3000 Hz. FFT-isolation på mindst −60 dB og latenstid under 40 ms i begge retninger.

## M4 – Per-app-routing til Discord
`IAudioPolicyConfigFactory` (Win10/11), verifikation med `IAudioSessionManager2`, gendannelse og guide som fallback.

## M5 – App og UI (Tauri 2 + React + TS)
Første opstart (velkommen → vi sætter op → klar), hovedskærm, avanceret mixer, indstillinger, hjælp/diagnose, systembakke, genvejstaster, autostart, da/en. Playwright-skærmbilleder.

## M6 – Installer og release
NSIS: én UAC, henter og installerer VB-CABLE, omdøber, ren afinstallation. Tauri updater, `SIGNING.md`, installationstest, `v1.0.0`.

## M7 – Reviews og afslutning
Fem uafhængige reviewroller, til to runder i træk er rene. README, `MANUAL_TEST.md`, 2-timers udholdenhed og ærlig slutopsummering.

## Valgfrit (kun hvis det ikke forsinker): to-kabel-tilstand
Findes et ekstra virtuelt kabel (f.eks. VB-CABLE A+B), tilbydes "Brug Claude i Chrome eller desktop-appen". Ellers er tilstanden skjult.
