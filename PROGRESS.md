# Fremdrift

Alle resultater kommer fra GitHub Actions på Windows-runnere. Rå resultater (JSON, logs, skærmbilleder) ligger på grenene `ci-results/<run>-<forsøg>-<job>`.

## M0 – Fundamentet: Discords virtuelle mikrofon ✔
[driver-proof #45783794886](https://github.com/clark-nielsen-gif/Claude-talk/actions/runs/45783794886): alle jobs grønne på `windows-latest`, `windows-2025` og `windows-2022`.
- VB-CABLE (Microsoft-attesteret, produktionssigneret) installeres lydløst uden driverdialog og uden genstart. Den består `signtool /kp`.
- Den virtuelle mikrofon omdøbes til **"Claude Talk – til Discord (VB-Audio)"** via Core Audio. Navnet holder efter en genstart af lydtjenesten.
- Afvist med bevis: VirtualDrivers (kun SignPath-signatur) og VB-Audio Hi-Fi Cable (krydssigneret, tvinger en sikkerhedsdialog frem).

## M1 – Lydmotoren ✔
- **Enhedstests og property-tests** (`ci`, hver push): routing-regler (ingen hører sig selv, ingen sløjfer, også efter vilkårlige ændringer), drift-regulering (2 timer simuleret ved ±500 ppm), adaptiv jitterbuffer, true-peak-limiter (2.000 property-tests mod en uafhængig referencemåler), ekkodæmpning (ekko −16,5 → −85,6 dB, din stemme bevares), og at lydtrådene aldrig allokerer hukommelse.
- **Rigtige Windows-enheder** (`audio-e2e`): kabel-rundtur (−10,5 dB nøjagtigt), process loopback isolerer ét program (anden lyd > 150 dB nede), et program, der lukkes og startes igen, findes automatisk igen, og mikrofonvalget vælger aldrig vores egen virtuelle mikrofon.

## M2 – Claude-vinduet ✔
- `bridge.js` er testet i Chromium mod en side med streng CSP: getUserMedia, enumerateDevices, permissions, Web Audio, MediaRecorder og WebRTC får mixet (1000 + 2500 Hz ved −12 dB, alt andet > 140 dB nede). Kloner af mikrofonsporet holder lyden i live, og først når alle er stoppet, frigives mikrofonen.
- I det rigtige program på Windows (`app-e2e` #45800732660): Claude-vinduet (WebView2) får tilladelse, getUserMedia virker, Discords lyd når siden, og Claudes stemme fanges isoleret fra WebView2-procestræet og når Discords mikrofon.

## M3 – Brugerfladen ✔
Forside med tre kort og stor kontakt, opsætning i 3 skærme, avanceret mixer (tilføj og fjern kilder og udgange, lydstyrke pr. felt, forudindstillinger), indstillinger, hjælp med selvtest. Dansk og engelsk, lyst og mørkt tema, 100 % og 150 %. Skærmbilleder af alle skærme og fejltilstande laves af Playwright (`ci` → job `ui`).

## M4 – Installation, opdatering, udgivelse (i gang)
- Installeren henter VB-CABLE fra vb-audio.com, tjekker pakkens SHA-256 og Microsofts signatur, installerer lydløst, omdøber enhederne og bevarer brugerens standardenheder. Afinstallation gendanner Discords mikrofon og fjerner kablet, hvis Claude Talk installerede det.
- `install-test` kører den rigtige `ClaudeTalk-Setup.exe` lydløst på en ren runner og måler opstart, CPU og RAM.
- Signerede opdateringer (Tauri updater) og release-workflow er klar. Hemmelighederne sættes efter `SIGNING.md`.

## Under arbejde
- `app-e2e` i to tilstande (høretelefoner og højttalere) med frekvensisolation, latens med toneblink i begge retninger og robusthed (kablet forsvinder og kommer igen, 44,1 kHz, genstart af lydtjenesten).
- Per-app-routing med falske Discord.exe og chrome.exe (`audio-e2e`, `per_app_routing` og `per_app_restore`).
- 2-timers udholdenhedstest (`endurance`, køres én gang til sidst).
- Review-runder (`REVIEWS.md`).
