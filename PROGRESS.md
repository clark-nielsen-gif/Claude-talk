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
- `install-test` ([#45851616080](https://github.com/clark-nielsen-gif/Claude-talk/actions/runs/45851616080), grøn) kører den rigtige `ClaudeTalk-Setup.exe` lydløst på en ren runner: opstart 0,4 s, CPU 2,0–2,8 % af hele maskinen (2 kerner) og RAM ca. 20 MB, når Claude Talk er startet med Windows (intet vindue åbent). Autostart kun for brugeren, med citeret sti. Ren afinstallation.
- Opdateringer er slået fra, indtil der er en signeringsnøgle (`SIGNING.md`, K17).

## Målinger mod kravene (seneste kørsler)

| Krav | Resultat | Kørsel |
|---|---|---|
| Discord hører Claude og dig, aldrig sig selv | ✔ Discords tone > 140 dB under (uden ekkodæmpning), > 90 dB (med) | `app-e2e` [#45851609329](https://github.com/clark-nielsen-gif/Claude-talk/actions/runs/45851609329) |
| Claude hører dig og Discord, aldrig sig selv | ✔ Claudes tone > 65 dB under på siden | samme |
| Robusthed (kabel væk og tilbage, 44,1 kHz, programmer og lydtjeneste genstartet) | ✔ alle trin | samme |
| Start under 2 s | ✔ 0,4 s | `install-test` [#45851616080](https://github.com/clark-nielsen-gif/Claude-talk/actions/runs/45851616080) |
| CPU under 3 % | ✔ 2,0–2,8 % (hele maskinen, i baggrunden) | samme |
| RAM under 150 MB | ✔ ca. 20 MB uden vindue. Med hovedvindue og Claude-vindue åbne kommer WebView2 oveni (ca. 200–270 MB for hele træet). | samme og `endurance` |
| Forsinkelse under 40 ms | ✘ Ikke dokumenteret opfyldt. Claude Talks egen del måles til 20 ms alene (`audio-e2e`), men til 60–68 ms i hele programmet uden ekkodæmpning og 88–100 ms med. Fra mikrofon til Discord i alt ca. 105–160 ms. Til Claude-siden 45–75 ms. Se K9, K25, K29. | `app-e2e` og `audio-e2e` [#45851622079](https://github.com/clark-nielsen-gif/Claude-talk/actions/runs/45851622079) |
| Stabil i 2 timer | ✔ for isolation, hukommelse og at programmet kører: 25 målinger over 120 min, isolation hver gang, hukommelse uden vækst. Kørslen er markeret som fejlet, fordi den også kræver under 40 ms til Claude (45–75 ms) | `endurance` [#45839660338](https://github.com/clark-nielsen-gif/Claude-talk/actions/runs/45839660338) |
| Review-runder (to rene i træk) | ✘ Ikke nået. 8 runder; design ren i runde 7 og 8, "min mor" ren i runde 7. Se `REVIEWS.md`. | |

## Tilbage
- Forsinkelsen (K29): årsagen til forskellen mellem måling alene og i hele programmet er ikke fundet.
- To rene review-runder i træk.
- Kodesignering (K26) og offentligt projekt (K17) kræver ejerens handling.
