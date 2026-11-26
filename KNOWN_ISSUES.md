# Kendte begrænsninger

## K1 – Windows 10 understøttes ikke
At optage ét programs lyd kræver Windows build 20348 eller nyere (`DECISIONS.md` D10). Windows 10 22H2 (19045) kan det ikke, så programmet viser beskeden "Claude Talk kræver Windows 11". **Bedste alternativ:** opdater til Windows 11. Der findes heller ingen Windows 10-runner i GitHub Actions. `windows-2022` (build 20348) dækker det ældste understøttede API-niveau.

## K2 – Runnere har test-signing slået til
Driverindlæsning på runneren beviser derfor ikke signaturen. Signaturen bevises i stedet med `signtool /kp` og Microsofts katalogsignatur på den installerede pakke (se `DECISIONS.md` D1). Secure Boot er slået fra på runnerne.

## K3 – Kun én gratis, produktionssigneret virtuel mikrofon findes – **LØST**
Løst med arkitektur 2 (`DECISIONS.md` D5): Kun Discord får en virtuel mikrofon (VB-CABLE). Claude får lyden direkte i Claude Talks indbyggede Claude-vindue. Har brugeren selv et ekstra virtuelt kabel (VB-CABLE A/B/C/D, VoiceMeeter eller Hi-Fi Cable), viser Indstillinger "Brug Claude i Chrome eller Claude-appen". Ellers er punktet skjult. I den tilstand går al lyd fra det valgte program til Discord, fordi Windows kun kan fange et helt programs lyd.

## K4 – Claudes stemmefunktion kan ikke testes med et rigtigt login i CI
CI kan ikke logge ind på claude.ai. Indsprøjtningen testes mod en lokal testside, der efterligner en stemme-webapp (getUserMedia, enumerateDevices, Web Audio, streng CSP). Hvis claude.ai ændrer, hvordan den henter mikrofonen, kan det kræve en opdatering. Det rigtige forløb står i `MANUAL_TEST.md`.

## K5 – "Fortsæt med Google" virker ikke i Claude-vinduet
Google blokerer login i indlejrede browsere. Brug login med e-mail i stedet. Vinduet viser en vejledning, når det sker.

## K6 – Mikrofonen i e2e-testen er simuleret
CI-runnere har ingen mikrofon, der kan spille en kendt tone. I e2e-testen leveres "mikrofonen" derfor som et testprograms lyd. Optagelse fra en rigtig optageenhed er testet separat (`audio-e2e`, `render_capture_cable`), og den rigtige mikrofon testes i `MANUAL_TEST.md`.

## K7 – Første opstart er langsommere end de næste
Første gang programmet starter på en pc, opretter WebView2 sin profil. På en ren CI-runner tog det ca. 8 s, før lydmotoren kørte. Næste start tog 0,7 s. Lyden starter efter hovedvinduet, fordi Tauri opretter vinduet først.

## K8 – Discord skal måske genstartes én gang
Windows' indstilling for mikrofon pr. program kan først sættes, når Discord har brugt lyd, og en optagelse, der allerede kører, flytter ikke altid med. Claude Talk prøver igen hvert andet sekund og viser "Genstart Discord for mig", hvis Discord stadig optager fra en anden mikrofon. Indstillingen huskes derefter af Windows.

## K9 – VB-CABLE lægger sin egen forsinkelse til
VB-CABLE har en intern buffer (standard 7168 samples). Kablet lægger målt ca. 35–40 ms til Discord-retningen ud over Claude Talks egen del (`app-e2e`, `vbcable_added_ms`). VB-Audio anbefaler kun eksperter at ændre den og dokumenterer ikke, hvor indstillingen gemmes. Claude Talk ændrer den derfor ikke. Claude Talks egen del måles og holdes for sig.

## K10 – Ekkodæmperen kører i sin egen tråd og allokerer
WebRTC AEC3-biblioteket (`aec3`) allokerer pr. blok. Det kører derfor ikke i en enhedstråd, men i en egen tråd med »Pro Audio«-prioritet, der læser mikrofonen fra en låsefri ring. En forsinket blok kan i værste fald give et kort udfald i mikrofonen, men aldrig i de andre lydveje.

## K11 – Afinstallation via en anden brugers administratorkode
Hvis en almindelig bruger afinstallerer med en administrators kode, rydder afinstallationen administratorens brugerindstillinger (autostart og Discords mikrofonvalg), ikke brugerens. Brugerens autostart peger så på et program, der ikke findes, og Windows ignorerer den. Discords mikrofonvalg peger på en enhed, der er fjernet, og Discord falder tilbage til standardmikrofonen.

## K12 – »Gendan normale lydindstillinger«, mens Discord er lukket
Windows' lydtjeneste husker Discords mikrofonvalg i hukommelsen, så det kan først ændres, når Discord kører. Claude Talk sletter det gemte valg med det samme og retter det, så snart Discord starter. Hvis Discord allerede er begyndt at optage, skal Discord genstartes én gang. Det viser Claude Talk selv.

## K13 – Aktiveringsordet er en instruktion til Claude
»Svar kun, når nogen siger …« bliver sendt til Claude som en instruktion. Claude følger den godt, men kan en sjælden gang svare alligevel. Skal Claude slet ikke høre noget før ordet, kræver det talegenkendelse på pc'en. Windows' indbyggede talegenkendelse kan ikke dansk og kan ikke lytte til Claude Talks egen lyd. »Claude hører kun mig« håndhæves derimod af Claude Talk selv. Hvis claude.ai ændrer sit beskedfelt, kan den automatiske afsendelse stoppe. Så viser Claude Talk en besked, og instruktionerne kan indsættes manuelt.

## K14 – Ekkodæmpningen skal lige finde sig til rette
Når ekkodæmpningen starter (Claude Talk opdager højttalere), tager WebRTC AEC3 ca. 6 sekunder om at se, at din stemme ikke er ekko. Taler Claude eller Discord i de sekunder, bliver din stemme dæmpet kraftigt. Derefter går den uændret igennem (målt i `aec.rs`-testene). AEC3's hurtigere HMM-tilstand forkorter det til 2 sekunder, men klipper så ca. 20 ms af hver sætningsstart, mens Claude taler, resten af opkaldet. Den er derfor fravalgt.
