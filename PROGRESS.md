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

## M4 – Installation, opdatering, udgivelse
- Installeren henter VB-CABLE fra vb-audio.com, tjekker pakkens SHA-256 og Microsofts signatur, installerer lydløst, omdøber enhederne og bevarer brugerens standardenheder rolle for rolle. Afinstallation gendanner Discords mikrofon og fjerner kablet, hvis Claude Talk installerede det. En opgradering rører ikke kablet (K27).
- `install-test` ([#45950436442](https://github.com/clark-nielsen-gif/Claude-talk/actions/runs/45950436442), grøn) kører den rigtige `ClaudeTalk-Setup.exe` lydløst på en ren runner: opstart 0,23 s, CPU 1,3 % af hele maskinen og RAM 20 MB, når Claude Talk er startet med Windows (intet vindue åbent). Driveren er produktionssigneret, kablet omdøbes og mærkes som Claude Talks. Autostart kun for brugeren, med citeret sti. Ren afinstallation (program, autostart, kabel og driverfiler væk).
- Opdateringer er slået fra, indtil der er en signeringsnøgle (`SIGNING.md`, K17).

## Målinger mod kravene (endelige kørsler på 33aa3ba, samme kode som de to rene review-runder)

| Krav | Resultat | Kørsel |
|---|---|---|
| Discord hører Claude og dig, aldrig sig selv | ✔ I Discords mikrofon: Claude og du ved −10,5/−10,9 dB, Discords egen lyd −153,6 dB (uden ekkodæmpning) og −124,8 dB (med), et andet program −145 dB | `app-e2e` [#45950428099](https://github.com/clark-nielsen-gif/Claude-talk/actions/runs/45950428099) |
| Claude hører dig og Discord, aldrig sig selv | ✔ På Claude-siden: du og Discord ved −10,5/−10,8 dB, Claudes egen stemme −162,3 dB (uden) og −135,9 dB (med) | samme |
| Robusthed (kabel væk og tilbage, 44,1 kHz, mikrofonprogram og lydtjeneste genstartet) | ✔ alle trin | samme |
| Start under 2 s | ✔ 0,23 s | `install-test` [#45950436442](https://github.com/clark-nielsen-gif/Claude-talk/actions/runs/45950436442) |
| CPU under 3 % | ✔ 1,3 % i baggrunden (hele maskinen). Med begge vinduer åbne og lyd i gang 2,3–5,7 % (`endurance`) | samme |
| RAM under 150 MB | ✔ 20 MB uden vindue. ✘ med hovedvindue og Claude-vindue åbne: ca. 305–316 MB for hele træet (WebView2) | samme og `endurance` |
| Forsinkelse under 40 ms | ✘ Ikke opfyldt. Til Claude-siden median 62 ms (uden ekkodæmpning) og 70 ms (med); Claude Talks egen andel mod Discord 56 ms og 86 ms. Se K9, K25, K29. | `app-e2e` [#45950428099](https://github.com/clark-nielsen-gif/Claude-talk/actions/runs/45950428099), `audio-e2e` [#45950433528](https://github.com/clark-nielsen-gif/Claude-talk/actions/runs/45950433528) |
| Stabil i 2 timer | Delvist ✔: 25 målinger over 120 min, programmet kørte hele vejen, isolationen holdt hver gang (Discord −148 til −154 dB, én gang −128 dB), hukommelsen flad (305–316 MB efter start). ✘ Forsinkelsen til Claude var 55–72 ms de første 80 min og steg derefter til 106–120 ms og blev der (K42). Kørslen er markeret som fejlet, fordi den kræver under 40 ms til Claude. | `endurance` [#45950430723](https://github.com/clark-nielsen-gif/Claude-talk/actions/runs/45950430723) |
| Review-runder (to rene i træk) | ✔ Runde 28 og 29 rene i alle fem roller (29 runder i alt). Se `REVIEWS.md`. | |
| Alle tests grønne | ✔ `ci`, `audio-e2e`, `install-test` grønne. `app-e2e` og `endurance` grønne på alt undtagen forsinkelseskravet. | |

## Tilbage
- Forsinkelsen (K29) og springet i forsinkelse til Claude efter lang tid (K42): årsagen er ikke fundet.
- RAM med begge vinduer åbne er over 150 MB (WebView2).
- Kodesignering (K26) og offentligt projekt (K17) kræver ejerens handling.
