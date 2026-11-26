# Claude Talk – plan

Hver milepæl afsluttes med grøn CI på Windows-runnere, en opdatering af `PROGRESS.md` og en review-runde (se `REVIEWS.md`).

## M0 – Fundament: virtuelle mikrofoner (bevises først)
- `driver-proof.yml`: test kandidat A (VirtualDrivers, signeret 25.7.14) og kandidat B (VB-CABLE + Hi-Fi Cable fra vb-audio.com) på `windows-latest`, `windows-2025` og `windows-2022`.
- Bevis: ingen test-signing, to aktive optageenheder, omdøbt til `Claude Talk – til Discord` / `Claude Talk – til Claude`, læst tilbage via Core Audio (`PKEY_Device_FriendlyName`).
- Bevis: lyd går igennem begge kabler (tone ind på render-siden, FFT på capture-siden).
- Bevis: afinstallation fjerner enhederne igen.
- **Stop-kriterie:** Ingen kode ud over M0, før dette er grønt.

## M1 – Lydmotor (Rust, `windows`-craten)
- WASAPI shared/event capture og render, MMCSS "Pro Audio", én tråd per enhed.
- Process loopback (`AUDIOCLIENT_ACTIVATION_TYPE_PROCESS_LOOPBACK`, include process tree) for Chrome/Claude og Discord, med automatisk genfinding af PID.
- Låsefri SPSC-ringbuffere, adaptiv resampling (`rubato`) mod clock drift, gain/mute, true-peak limiter, målere.
- Routing-graf med løkkedetektion og hårde låse (`Claude → Til Claude`, `Discord → Til Discord`).
- Tests: unit + `proptest` for mixer, gain, limiter, resampler, drift, graf.

## M2 – End-to-end lydtests i CI
- Testprogrammer: falsk Claude (440 Hz), falsk Discord (1000 Hz), falsk mikrofon (2500 Hz via ekstra virtuel enhed).
- FFT-isolation (≥ 60 dB), latenstid < 40 ms med impulser, robusthed (genstart af processer, enheder fra/til, sample rate-skift).
- Separat 2-timers udholdenhedsworkflow.

## M3 – Per-app-routing
- `IAudioPolicyConfigFactory` (Win10- og Win11-GUID'er) for Discord/DiscordPTB/DiscordCanary og chrome/Claude.
- Verifikation med `IAudioSessionManager2`, "Gendan normale lydindstillinger", test med omdøbte testprocesser.

## M4 – App (Tauri 2 + React + TS + Vite)
- Første opstart (3 skærme), hovedskærm (Dig / Claude / Discord), avanceret mixer, indstillinger, hjælp/diagnose, systembakke, genvejstaster, autostart, i18n (da/en).
- Design efter afsnit 6, eget ikon, bundlede skrifttyper.
- Playwright-skærmbilleder: alle skærme, lys/mørk, 100 %/150 %.

## M5 – Installer, opdatering, release
- NSIS: én UAC, henter og installerer driverne, omdøber enheder, ren afinstallation.
- Tauri updater med signerede manifester, kodesignering forberedt (`SIGNING.md`).
- Installationstest i CI, `v1.0.0`-release med `ClaudeTalk-Setup.exe` + portable zip.

## M6 – Review-runder til to rene runder i træk
- Kodereviewer, lydtester, designreviewer, "min mor"-tester, "bryd det"-agent.
- README (dansk, ikke-teknisk), `MANUAL_TEST.md`, ærlig slutopsummering i `PROGRESS.md`.
