# Beslutninger

## D1 – Virtuelle lyddrivere (bevist i CI, se `driver-proof.yml` og grenene `ci-results/*`)

### Vigtigt om CI-beviset
GitHubs Windows-runnere starter med **test-signing slået til** (det er billedets standard, og det kan ikke slås fra uden en genstart, som en hostet runner ikke overlever). At en driver installerer på runneren, beviser derfor intet. Produktionssignatur bevises i stedet ud fra signaturkæden på den pakke, Windows faktisk har lagt i DriverStore: `signtool verify /kp` (Windows' kernepolitik) skal bestå, og kataloget skal være signeret af Microsoft (attestation/WHQL).

### Kandidat A: VirtualDrivers "Virtual Audio Driver" 25.7.14 – **afvist**
- Signeret af "SignPath Foundation" (Authenticode). `signtool /kp` fejler. Ingen Microsoft-attestation, ingen WHQL. Kræver test-signing på en almindelig pc.
- INF'en er målrettet `NT$ARCH$.10.0...22000` (kun Windows 11). Én mikrofon. Navnet ligger i INF'en.

### Kandidat B1: VB-CABLE (gratis, `VBCABLE_Driver_Pack46.zip`) – **godkendt**
Bevist på `windows-latest`, `windows-2025` og `windows-2022`:
- Installeret pakke: `vbmmecable64_win10.inf`, version 3.5.2.0 (31-10-2026). Katalogsigner: **Microsoft Windows Hardware Compatibility Publisher**, attribut **Attested**. `signtool /kp` består. Det er en produktionssignatur.
- `VBCABLE_Setup_x64.exe -i -h` installerer lydløst **uden** driverdialog og uden at udgiveren er betroet på forhånd. Afslutningskode 0, enheder aktive uden genstart.
- Mikrofonen kan omdøbes via Core Audio-API'et (`IMMDevice::OpenPropertyStore(STGM_READWRITE)`), samme mekanisme som Windows' Lyd-kontrolpanel. Ingen registry-hacks og intet SYSTEM-trick nødvendigt. Navnet overlever en genstart af lydtjenesten.
- **Navnet bliver "Claude Talk – til Discord (VB-Audio)".** Windows sammensætter altid det synlige navn som "beskrivelse (grænseflade)". At skrive det sammensatte navn direkte afvises med `E_ACCESSDENIED` (bevist i CI), og en tom grænseflade giver "()". "(VB-Audio)" er pænt og opfylder samtidig VB-Audios licensbetingelse om, at brugeren kan se, at kablet er fra VB-Audio.
- Windows Server-billeder har lydtjenesten slået fra. Den startes i CI. På almindelige Windows-pc'er kører den altid.

### Kandidat B2: VB-Audio Hi-Fi Cable (gratis, 2014) – **afvist**
- Installerer `vbhfvaio64_win7.inf`, som er signeret med VB-Audios gamle certifikat fra 2014 (krydssigneret, ikke Microsoft-signeret). Windows viser altid "Windows-sikkerhed"-dialogen (`pnpui.dll,InstallSecurityPromptRunDllW`), og installeren hænger, til nogen klikker. Dokumenteret med processer og skærmbilleder.
- Opfylder ikke kravet "Microsoft-attestationssigneret eller WHQL".
- Microsoft fjerner desuden standardtilliden til krydssignerede kernedrivere fra og med april 2026-opdateringen (Windows 11 24H2/25H2/26H1, Server 2025). Kun en liste over udvalgte, kendte drivere undtages. Hi-Fi Cable kan derfor holde op med at virke uden varsel.

### Kandidat B3: VB-CABLE A+B / C+D – **kan ikke hentes automatisk**
Kan kun hentes via et personligt link efter en donation i VB-Audios webshop ("pay what you want"). Installeren kan derfor ikke hente dem selv fra vb-audio.com.

### Konsekvens
Der findes præcis **én** virtuel mikrofon, der både er gratis, produktionssigneret og kan hentes automatisk: VB-CABLE. Løst med arkitektur 2 (D5): kun Discord får en virtuel mikrofon.

## D2 – CI-runnere
- `windows-latest` (er p.t. Server 2025, build 26100), `windows-2025` og `windows-2022` (build 20348, nærmeste til Windows 10). Der findes ingen hostet Windows 10-klient.

## D3 – CI-resultater læses via git
Udviklingsmiljøet kan ikke hente GitHub Actions-artefakter (proxyen blokerer Azure-blob-lageret). Hvert job lægger derfor sine resultater (JSON, logs, skærmbilleder) på sin egen gren `ci-results/<run>-<forsøg>-<job>`, som hentes med `git fetch`. Grenene er kun bevismateriale og kan slettes uden konsekvens.

## D4 – Repository
`clark-nielsen-gif/Claude-talk` (privat, oprettet af brugeren). Bemærk: Private repositories har en begrænset kvote af gratis Actions-minutter, og Windows-minutter tæller dobbelt. 2-timers udholdenhedstesten bruger en mærkbar del af kvoten.

## D5 – Arkitektur 2: Claude får lyd direkte i et indbygget vindue (brugerens valg 26-11-2026)
- Discord bruger VB-CABLE som mikrofon (Mikrofon + Claude).
- Claude kører i et Claude Talk-vindue (WebView2). Et script, der injiceres før siden indlæses, erstatter `navigator.mediaDevices.getUserMedia`, `enumerateDevices` og `permissions.query` for mikrofonen. claude.ai får dermed en MediaStream med Mikrofon + Discord fra Rust-motoren.
- **Claudes stemme isoleres med process loopback på Claude-vinduets eget WebView2-processtræ.** Vinduet får sit eget WebView2-miljø (egen brugerdatamappe), så det får sin egen browserproces (`ICoreWebView2::BrowserProcessId`) og sin egen lyd-tjenesteproces, adskilt fra programmets UI. Process loopback med `INCLUDE_TARGET_PROCESS_TREE` på den PID fanger kun Claude-vinduets lyd. Brugeren hører stadig Claude normalt, fordi intet omdirigeres.

## D6 – Er Claudes stemmefunktion tilgængelig i en Chromium-browser? Ja
Anthropics hjælpeside (support.claude.com, "Use voice mode", tjekket 26-11-2026) siger, at voice mode er en betafunktion for alle abonnementer på mobil, Claude Desktop og nettet. Claude Desktop er selv bygget på Chromium (Electron), og WebView2 er Microsoft Edge' Chromium-motor med `getUserMedia`, WebRTC, Web Audio og WebCodecs. Der er ikke fundet krav, som WebView2 mangler. Det endelige bevis kan kun gives med et rigtigt login, så det er et trin i `MANUAL_TEST.md`. Risikoen står i `KNOWN_ISSUES.md` K4.

## D7 – Login
- Google blokerer OAuth i indlejrede browsere med fejlen `disallowed_useragent`. Det har Google håndhævet siden 2021. "Fortsæt med Google" forventes derfor at fejle i vinduet. Vinduet opdager navigation til `accounts.google.com` og viser i stedet en venlig vejledning: "Log ind med din e-mail – Claude sender dig en kode."
- Login sker udelukkende på Anthropics egne sider. Programmet læser, gemmer og rører aldrig adgangskoder, koder eller cookies. Sessionen ligger kun i WebView2's egen brugerdatamappe (`%APPDATA%\Claude Talk\ClaudeWindow`).
- Tilladt i vinduet: `claude.ai`, `*.claude.ai`, `*.anthropic.com`, `*.claude.com` og Cloudflares udfordringsside (`challenges.cloudflare.com`, som claude.ai selv bruger). Alt andet åbnes i standardbrowseren.

## D8 – Lydtransport fra motoren til siden
**Valg:** WebView2-hostbeskeder (`PostWebMessageAsString` → `window.chrome.webview`), i blokke på 10 ms (480 frames, 48 kHz mono f32, base64), og i siden en `MediaStreamTrackGenerator` (WebCodecs `AudioData`). Fallback er en `AudioWorklet` og til sidst en `ScriptProcessorNode`.

**Begrundelse:**
- En lokal WebSocket er underlagt sidens CSP (`connect-src`). claude.ai har en streng CSP, så `ws://127.0.0.1` ville blive blokeret. Hostbeskeder er ikke underlagt CSP og forlader aldrig processen, så de kræver ingen port eller token.
- En `AudioWorklet` skal indlæses med `addModule(url)`, og det er underlagt `script-src`. Med en nonce-baseret CSP er `blob:`- og `data:`-moduler blokeret. `MediaStreamTrackGenerator` kræver ingen modulindlæsning, og frames skrives direkte ind i sporet, så der ikke er nogen ekstra jitterbuffer ud over den, Chromium selv har.
- Shared buffers (`PostSharedBufferToScript`) kan ikke overføres til en lydtråd i siden og skulle polles fra hovedtråden. Det giver mere jitter, ikke mindre.
- Det endelige valg verificeres i CI med testsiden, som har samme type CSP som claude.ai, og latenstiden måles.

## D9 – CI-minutter (privat repository)
Hurtige tests kører kun på `windows-latest`. Matrixen køres kun ved milepæle, og udholdenhedstesten (2 timer) køres én gang til sidst. Ren DSP-logik testes desuden på Linux i udviklingsmiljøet, hvilket er gratis.

## D10 – Windows 11 er et krav (process loopback)
Optagelse af ét programs lyd med hele dets procestræ (`PROCESS_LOOPBACK_MODE_INCLUDE_TARGET_PROCESS_TREE`) kræver Windows build 20348 eller nyere. Det svarer til Windows 11 og Server 2022. Windows 10 22H2 er build 19045 og understøttes derfor ikke. Programmet viser i så fald en venlig besked ("Claude Talk kræver Windows 11") i stedet for at starte lyden. Process loopback er ikke bundet til en bestemt udgang, så Discords lyd fanges også, når Discord spiller på et headset, der ikke er standardenheden.

## D11 – Realtidsdesign af lydmotoren
- Hver route (kilde → udgang) har sin egen låsefri SPSC-ring og sin egen resampler. Derfor kører hver udgang på sit eget ur, helt uden låse. Routes tilføjes og fjernes via en kommandokø, og frigivne objekter sendes tilbage til kontroltråden, så lydtråden aldrig frigiver hukommelse. Det er bevist med en test, der tæller allokeringer (`engine/tests/realtime.rs`).
- WASAPI leverer altid 48 kHz float stereo (`AUTOCONVERTPCM`). Resampleren (`rubato`, kubisk polynomiel, 1 ms-chunks) håndterer derfor kun clock drift.
- **Drift-regulering:** PI-sløjfe med naturlig frekvens 1,5·10⁻³ rad pr. blok og dæmpning ≈ 0,7. Fyldningen midles over 2 s, P-leddet er begrænset til ±1000 ppm og det samlede udsving til ±2500 ppm, så ingen korrektion kan høres. Resultatet er målt i tests: −557 ppm ramt med under 5 ppm fejl og under 200 ppm rippel ved 22 ms-ryk.
- **Adaptiv jitterbuffer:** Hver route starter med 20 ms. Den vokser 5 ms ved hver underrun (højst 60 ms) og skrumper 1 ms pr. minut uden problemer. Et stort efterslæb (f.eks. ved opstart) kasseres med det samme i stedet for at blive indhentet langsomt.
- **Fraværende kilder leverer stilhed i realtid:** Når Discord lukkes, eller en mikrofon tages ud, holdes routes kørende med stilhed. Så vokser bufferen og latenstiden ikke, når kilden vender tilbage.
- **True-peak-limiter pr. udgang:** 8× polyfase-estimat og sliding minimum med boksfilter. Ingen sample overskrider loftet. Med 0,3 dB intern margen holdes −1 dBTP mod en uafhængig 16× referencemåler (2.000 property-tests).
- Lydtrådene logger aldrig. De publicerer tilstand via atomics, som kontroltråden logger.

## D12 – Ekko og "egen kabel"-sikring
Mikrofonkilden springer altid Claude Talks egne virtuelle enheder over, også hvis Windows' standardmikrofon er VB-CABLE. Ellers ville "Til Discord" løbe ind i sig selv. Det er testet i CI (`microphone_never_own_cable`).

## D13 – Sådan testes det, CI ikke kan gøre "rigtigt"
- **claude.ai:** En testside med streng nonce-CSP efterligner en stemme-webapp (getUserMedia med typiske constraints, enumerateDevices, permissions, Web Audio, MediaRecorder og WebRTC-loopback). Den kører i det rigtige Claude-vindue i en CI-build med featuren `ci-testpage`. Den feature findes ikke i release-builds.
- **Mikrofon i e2e-testen:** Runnerne har ingen mikrofon, der kan afspille en kendt tone (testdriverens mikrofon er stum, målt i CI). I e2e-testen er "mikrofonen" derfor et testprograms lyd (process loopback). Optagelse fra en rigtig optageenhed er bevist separat i `audio-e2e` ved at optage VB-CABLE.
- **Testudgang:** De falske programmer spiller ud på en ekstra virtuel enhed (VirtualDrivers-driveren). Den kan kun installeres, fordi runnerne har test-signing slået til, og den bruges kun som CI-fixture.
