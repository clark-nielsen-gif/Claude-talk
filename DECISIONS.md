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
- Login sker udelukkende på Anthropics egne sider. Programmet læser, gemmer og rører aldrig adgangskoder, koder eller cookies. Sessionen ligger kun i WebView2's egen brugerdatamappe (`%LOCALAPPDATA%\Claude Talk\ClaudeWindow`; browserdata skal ikke følge med en roaming-profil).
- Tilladt i vinduet: `claude.ai`, `*.claude.ai`, `*.anthropic.com`, `*.claude.com` og Cloudflares udfordringsside (`challenges.cloudflare.com`, som claude.ai selv bruger). Alt andet åbnes i standardbrowseren.

## D8 – Lydtransport fra motoren til siden
**Valg:** WebView2-hostbeskeder (`PostWebMessageAsString` → `window.chrome.webview`), i blokke på 10 ms (480 frames, 48 kHz mono f32, base64), og i siden en `MediaStreamTrackGenerator` (WebCodecs `AudioData`). Fallback er en `ScriptProcessorNode`.

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
- **Adaptiv jitterbuffer:** Hver route starter med 20 ms. Den vokser 5 ms ved hver underrun (højst 40 ms). Efter 15 s uden problemer skrumper den 1 ms, men kun hvis der var mindst 4 ms tilbage efter hver aflæsning, og aldrig under det niveau, der sidst løb tørt (det gulv sænkes 5 ms efter 10 minutter, hvor der hele tiden var mindst et trin til overs; testen `buffer_comes_back_down_after_bursts`). En kilde, der genstarter (enheden åbnes igen, programmet kommer tilbage), får ikke bufferen til at vokse. En simulering af to timer med 10 ms-pakker og ±2 ms jitter (`resample.rs`, testen `jitter_does_not_keep_dropping_out`) giver ingen udfald ved 20 ms og gentagne udfald under det. Huller tones 1 ms ud og ind, så de ikke klikker. Et stort efterslæb (f.eks. ved opstart) kasseres med det samme i stedet for at blive indhentet langsomt.
- **Ekkodæmperens reference har fast forsinkelse:** Referencen vokser ikke og fyldes ikke op igen efter et hul (hullet bliver stilhed), så AEC3 ikke skal finde forsinkelsen igen. Den går ikke gennem limiteren, fordi ekkodæmperen skal have den rene sum.
- **Fraværende kilder leverer stilhed i realtid:** Når Discord lukkes, eller en mikrofon tages ud, holdes routes kørende med stilhed. Så vokser bufferen og latenstiden ikke, når kilden vender tilbage.
- **True-peak-limiter pr. udgang:** 8× polyfase-estimat og sliding minimum med boksfilter. Ingen sample overskrider loftet. Med 0,3 dB intern margen holdes −1 dBTP mod en uafhængig 16× referencemåler (2.000 property-tests).
- Lydtrådene logger aldrig. De publicerer tilstand via atomics, som kontroltråden logger.

## D12 – Ekko og "egen kabel"-sikring
Mikrofonkilden springer altid Claude Talks egne virtuelle enheder over, også hvis Windows' standardmikrofon er VB-CABLE. Ellers ville "Til Discord" løbe ind i sig selv. Det er testet i CI (`microphone_never_own_cable`).

## D13 – Sådan testes det, CI ikke kan gøre "rigtigt"
- **claude.ai:** En testside med streng nonce-CSP efterligner en stemme-webapp (getUserMedia med typiske constraints, enumerateDevices, permissions, Web Audio, MediaRecorder og WebRTC-loopback). Den kører i det rigtige Claude-vindue i en CI-build med featuren `ci-testpage`. Den feature findes ikke i release-builds.
- **Mikrofon i e2e-testen:** Runnerne har ingen mikrofon, der kan afspille en kendt tone (testdriverens mikrofon er stum, målt i CI). I e2e-testen er "mikrofonen" derfor et testprograms lyd (process loopback). Optagelse fra en rigtig optageenhed er bevist separat i `audio-e2e` ved at optage VB-CABLE.
- **Testudgang:** De falske programmer spiller ud på en ekstra virtuel enhed (VirtualDrivers-driveren). Den kan kun installeres, fordi runnerne har test-signing slået til, og den bruges kun som CI-fixture.

## D14 – Ekkodæmpning uden ekstra forsinkelse
Ekkodæmperen (AEC3) læser mikrofonen direkte fra optageringen på mikrofonens eget ur, altså uden ekstra jitterbuffer. Referencen (Claude + Discord) hentes én gang pr. mikrofonblok med en kort buffer på 20 ms (10 ms løb tør, se D21), så den normalt når frem før ekkoet (højttaler- og mikrofonlatens er længere). Ekkodæmpningen lægger dermed kun én blok på 10 ms til mikrofonens vej. Den første version havde to jitterbuffere og kostede ca. 20 ms mere.

## D15 – Sådan måles latens
- **Mikrofon → Discord:** Den falske mikrofon spiller korte toneblink (2500 Hz). Testværktøjet optager samtidig programmets lyd (process loopback) og Discords virtuelle mikrofon på ét fælles ur. Hver optagelse korrigeres for testværktøjets egen buffer. Forsinkelsen er medianen af forskellene mellem blinkenes start. Begge sider bruger samme båndpasfilter, så filterets forsinkelse går ud.
- **Discord → Claude:** Den falske Discord blinker ved 1000 Hz. Testværktøjet finder blinkenes vægur-tid i sin optagelse, og testsiden i Claude-vinduet registrerer ankomsttiden (`performance.timeOrigin + performance.now()`) med samme båndpasfilter. Siden måler med en lille buffer og tidsstempler efter lyden er kommet frem, så målingen kan kun overvurdere forsinkelsen.
- Begge veje måles i både høretelefon- og højttalertilstand (ekkodæmpning fra og til). Grænsen er 40 ms.

## D16 – Toneniveau måles med ±0,5 % søgning
Drift-reguleringen må med vilje rette op til ±0,25 % (ca. 4 cent, uhørligt). Testprogrammerne laver desuden deres toner efter vægur, men afspiller dem på en virtuel CI-enhed med sit eget ur. Det flytter tonen yderligere (målt op til ca. 0,2 %). En 2500 Hz-tone kan derfor ligge 5–6 Hz ved siden af. Niveaumålingerne (CI og selvtesten i programmet) søger derfor ±0,5 % rundt om tonen, dog mindst 3 Hz. Rapporterne viser også, hvor tonen blev fundet. Tonehøjdens stabilitet i selve motoren testes separat i `drift.rs`/`resample.rs`.

## D17 – 1 ms timeropløsning
Alle tråde, der tager tid efter uret (stilhed for ventende kilder, testtoner, ekkodæmpning og Claude-broen), kalder `timeBeginPeriod(1)` én gang pr. proces. Ellers kan en kort pause på Windows vare 15,6 ms, og lyden kommer i ryk.

## D18 – Mikrofonen er kun åben, når nogen kan høre den
Mikrofonen åbnes kun, mens Discord optager (i et opkald), Claude-siden bruger mikrofonen, Claude kører i et andet program, eller Claude Talks vindue er åbent (dets »Vi kan høre dig«-måler). Ellers er enheden lukket. Windows' mikrofonikon slukker, og Bluetooth-headsets skifter ikke til den dårlige håndfri-lyd. Mens mikrofonen er lukket, leverer motoren stilhed i realtid, så alt andet kører videre.

## D19 – Genvejstaster bruger F-taster
Ctrl+Alt+bogstav er det samme som AltGr+bogstav på danske tastaturer (Ctrl+Alt+M skriver µ). Standardtasterne er derfor Ctrl+Shift+F9 (gør Claude stille i Discord), Ctrl+Shift+F10 (min mikrofon) og Ctrl+Shift+F11 (hold for at tale til Claude). F12 bruges ikke: Windows reserverer den til fejlfinding, så den kan ikke registreres som genvej. Tastvælgeren tager kun bogstaver, tal, F-taster og mellemrum.

## D20 – Installerens tillidskæde
VB-CABLE hentes til en mappe under Program Files, hvor kun administratorer kan skrive. Pakken accepteres kun, hvis dens SHA-256 er den testede, eller hvis driverkataloget er signeret af »Microsoft Windows Hardware Compatibility Publisher«. Installationsprogrammet skal desuden være gyldigt signeret. Ved afinstallation køres det kun, hvis det har samme SHA-256 som ved installationen. Claude Talk omdøber og fjerner kun et VB-CABLE, som det selv har installeret. Et VB-CABLE, brugeren havde i forvejen, bruges med sine egne navne.

## D21 – Ekkodæmperens reference
Referencen er alle kilder, der ikke er brugeren: Claude, Discord, Claude i et andet program og tilføjede programmer. Ekkodæmperen genopbygges, hver gang sådan en kilde kommer eller går. Referencen har en buffer på 20 ms, og dens drift-regulering styrer efter forsinkelsen (fyldning plus hvor længe siden kilden sidst leverede), ikke efter fyldningen alene, som hopper en hel pakke, når de to ures faser krydser. Med 50–200 ppm mellem urene er det værste 2-sekunders vindue nu 21–49 dB (før 1–18 dB). Ved huller: et hul afspilles som stilhed, og den samme bid springes over, når den kommer, så forsinkelsen ikke flytter sig. Stilheden regnes fra der, hvor den sidste pakkes lyd sluttede (WASAPI's tidsstempel), ikke fra hvornår pakken kom, og gælden betales tilbage ned til det niveau, reguleringen holder (mål + en halv blok − kildens alder), ikke ned til målet (runde 8). Med 10 ms løb referencen tør flere gange i sekundet, og kun 4–27 dB ekko blev fjernet. Med 20 ms fjernes typisk 30–65 dB i en simulering af den rigtige kobling (testen `echo_is_removed_with_the_host_wiring`); når urene driver meget (−150 ppm), svinger det ned til ca. 12 dB i perioder. Kommer en bid aldrig (kilden genstartede, data gik tabt), opgives gælden efter tre aflæsninger, og referencen fyldes op til målet igen, så forsinkelsen er den samme som før hullet (testen `fixed_route_recovers_from_lost_and_late_input`). Referencen kommer derved ca. 30 ms efter lyden spilles, hvilket stadig er før ekkoet når mikrofonen gennem højttaler, luft og mikrofonens egen buffer.

## D22 – Instruktioner til Claude
Brugeren kan bestemme, hvordan Claude opfører sig i opkaldet: aktiveringsord, korte svar, dansk, »flere personer taler« og egne instruktioner. De bliver til én besked, som Claude Talk skriver ind i Claudes beskedfelt og sender, når en ny samtale starter (`/new`), eller med »Send til Claude nu«. Sidens script rører kun beskedfeltet og send-knappen og læser intet på siden.
- **Følges af Claude:** Aktiveringsordet er en instruktion. Claude følger den godt, men ikke garanteret. Et ægte lokalt aktiveringsord, hvor lyden slet ikke når Claude før ordet, kræver talegenkendelse på pc'en (se K13).
- **Håndhæves af Claude Talk:** »Claude hører kun mig« er routing. Discords lyd sendes slet ikke til Claude, uanset hvad Claude gør. Med højttalere kan de andres stemmer stadig nå mikrofonen gennem luften. Derfor slås ekkodæmpningen altid til i den situation (også hvis den er slået fra), og den fjerner Discords lyd fra mikrofonen. Det er god dæmpning, ikke 100 %.
- **Test:** Playwright-testen `instructions are written into the message field and sent` tjekker leveringen mod en side, der efterligner Claudes beskedfelt. Det rigtige forløb står i `MANUAL_TEST.md` trin 13.

## D23 – Windows' standardudgang må aldrig være vores kabel
Vælger nogen »Claude Talk – intern« som højttaler i Windows (det står i lydmenuen ved uret), sender Discord alt, hvad pc'en spiller, til alle i opkaldet, og brugeren hører intet. Claude Talk tjekker derfor hvert andet sekund standardudgangen for alle tre roller (standard, multimedier, kommunikation). Er den vores kabel, eller i Chrome-tilstand det ekstra kabel, sættes den sidste rigtige enhed tilbage, og startsiden viser »Vi har sat din lyd tilbage«. Installeren genskaber også standardenhederne rolle for rolle, så en separat kommunikationsenhed bevares.

## D24 – Autostart skrives af Claude Talk selv, kun for brugeren
Autostart-pluginnet skriver til HKLM (alle brugere), når programmet kører som administrator, og citerer ikke stien. Claude Talk skriver derfor selv `"C:\Program Files\Claude Talk\claude-talk.exe" --minimized` i HKCU\…\Run. Slår brugeren autostart fra i Jobliste, respekteres det. Afinstallationen fjerner poster i både HKCU og HKLM.

## D25 – Hovedvinduet oprettes først, når det åbnes
Startet med Windows (`--minimized`) opretter Claude Talk ikke hovedvinduet. Så kører der ingen WebView2-processer, før brugeren åbner vinduet fra ikonet ved uret. Det sparer hukommelse og CPU, mens ingen kigger. Lyden og Discords mikrofon kører uafhængigt af vinduet.

## D26 – Afslutning midt i et opkald
Discord bliver ved med at optage fra kablet, til Discord genstarter. Lukker brugeren Claude Talk, mens Discord optager fra kablet, spørger Claude Talk derfor: »Luk, og genstart Discord«, »Luk uden at genstarte« eller »Annuller«. Hvis hovedtråden går ned, gives Discords mikrofon tilbage. Når Windows lukker sessionen, gives den kun tilbage, hvis Claude Talk ikke starter med Windows igen (Run-værdien og Windows' egen startprogram-kontakt læses); ellers bevares den, så Discord ikke skal genstartes hver dag.

## D27 – Installerens ekstra sikkerhed
Kun et installeret Claude Talk (under Program Files) må bede om administratorrettigheder for at installere kablet. En bærbar kopi i en mappe, brugeren kan skrive i, viser i stedet »brug installeren«. Den testede pakke kendes på sin SHA-256 og tjekkes uden tilbagekaldelse (en firewall kan blokere tilbagekaldelsesserverne). Er pakken en anden end den testede, tjekkes signaturerne med tilbagekaldelse, installationsprogrammet skal være signeret af VB-Audio, driverkataloget af Microsofts hardwareprogram, og alle andre filer end driverfilerne (inf, cat, sys) og installationsprogrammet slettes, før det køres som administrator (så en DLL ved siden af ikke kan indlæses med det). Er kablet installeret, men slået fra i Windows, installeres det ikke igen (kode 7), og mærket røres ikke. På Windows 10 installeres intet (kode 8). SHA-256 beregnes i programmet. Mærket »installeret af Claude Talk« skrives først, når kablets enheder findes. De forhøjede tilstande logger i Program Files, aldrig i brugerens %APPDATA%.

## D28 – Hvilken mikrofon der er »dig«
Claude Talk optager som udgangspunkt Windows' standardmikrofon. Mange har valgt en bestemt mikrofon i Discord, netop fordi standarden er en anden (webcam, bærbarens mikrofon). Når Discord optager fra en bestemt mikrofon, og brugeren ikke selv har valgt en i Claude Talk, overtager Claude Talk den mikrofon (gemt i indstillingerne), før brugeren bliver bedt om at sætte Discord på »Standard«. Så hører de andre i opkaldet den samme mikrofon som før. Det gælder også en mikrofon, brugeren har sat for Discord under Windows' app-indstillinger. Under **Indstillinger → Din mikrofon** kan brugeren vælge en anden eller gå tilbage til Windows' standard; et valg her (også »Windows' standard«) overtages aldrig fra Discord igen; vejledningen og feltet **Dig** viser, hvilken mikrofon der bruges. Er den valgte mikrofon taget ud, bruges standarden, indtil den kommer igen. Claude Talks egne enheder kan aldrig vælges.

Har brugeren sat Discords mikrofon under Windows' »Lydstyrke og enhedsindstillinger for apps«, gemmes den værdi, før Claude Talk peger Discord på kablet, og den sættes tilbage i stedet for at blive ryddet (også efter en genstart af Claude Talk). Havde brugeren ingen app-indstilling, men har Claude Talk en mikrofon for »dig« (overtaget fra Discord eller valgt), får Discord den mikrofon som app-indstilling, når Claude Talk giver slip (Gendan, Luk, afinstallation). Ellers ville Discord på »Standard« få Windows' standardmikrofon, netop den brugeren havde fravalgt. Hjælpeprocessen og afinstallationen læser værdien fra indstillingerne. Er Windows' gemte værdi tom ved næste start (den blev fjernet, mens Discord var lukket), beholdes den gemte. Er Discord ikke startet, når Claude Talk lukker, fjernes kun Discords poster, der peger på kablet, i registreringsdatabasen (andre programmers brug af et VB-CABLE, brugeren havde i forvejen, røres ikke); hjælpeprocessen sætter den rigtige værdi, når Discord starter. Ved afinstallation med Discord lukket er der ingen hjælpeproces; README beder brugeren tjekke Discords inputenhed.

»Discord skal lige genstartes« foreslås kun, når Discord optager fra Windows' standardmikrofon (eller sin gemte app-indstilling), dvs. står på »Standard«. Har brugeren valgt en bestemt mikrofon i Discord, hjælper en genstart ikke, og vejledningen vises med det samme.

## D29 – Tidslinjen for programlyd
Lyd fra et program (Claude, Discord) kommer i pakker med Windows' tidsstempel (QPC). Claude Talk holder styr på, hvor langt den lyd, der er sendt videre, rækker i den tid: pakker og den stilhed, der er sat ind for dem. Pakke efter pakke følger tidslinjen antallet af prøver (Windows' tidsstempler kan svinge en brøkdel af et millisekund), trukket 5 % mod stemplerne, så den følger uret. Kun efter indsat stilhed, eller når et stempel springer 3 ms eller mere, sammenlignes stempler: en sen pakke, der overlapper stilhed, der allerede er sendt, får overlappet skåret af; en pakke, der helt dækkes, ændrer intet; et hul efter tabte pakker fyldes med stilhed (højst 100 ms). Stilheden fyldes kun op til 10 ms før nu, så en pakke, der kommer nu, ikke får sin start skåret af (runde 10). Overgange til og fra stilhed tones over 1 ms. Optagebufferen er 100 ms, så en sen opvågning ikke taber noget (i event-tilstand koster det ingen forsinkelse). Pakker uden tidsstempel tælles og rapporteres i CI. Testen `late_packets.rs` kører en model af optagesløjfen (en kopi af logikken i `run_capture`, ikke selve Windows-koden) med seks forskellige tidsforløb af elleve scenarier inkl. tabte pakker; plus tidsstempler, der svinger ±0,7 ms. Den kræver, at referencens forsinkelse aldrig springer mere end 1,5 ms ad gangen og højst svinger 4 ms over et helt forløb; det målte er højst 1,3 ms og 3,8 ms (pauser flytter forsinkelsen langsomt ca. 2,5 ms, som ekkodæmperen følger).
