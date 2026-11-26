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
Claude Talks egen del af vejen til Discord er målt til 20 ms (`audio-e2e`, `latency_engine_pipeline`, `engine_ms`) og er under målet på 40 ms. VB-CABLE lægger sin egen buffer oveni: på CI-runnerne målt mellem ca. 20 og 96 ms. Indstillingen ligger i `HKLM\SOFTWARE\VB-Audio\Cable\VBAudioCableWDM_Latency` (standard 7168 samples). Vi prøvede 2048, 1024 og 512 med genstart af driveren ([kørsel 45826851380](https://github.com/clark-nielsen-gif/Claude-talk/actions/runs/45826851380)): kablets forsinkelse var stadig 85–95 ms. Indstillingen ændrer altså ikke forsinkelsen mærkbart, og Claude Talk lader den være. Den samlede forsinkelse fra din mikrofon til Discord er derfor typisk 40–120 ms. Til samtale mærkes det som en kort pause, ikke som ekko.

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

## K15 – VoiceMeeter kan ikke bruges til »Claude i Chrome eller Claude-appen«
VoiceMeeter er en mixer med egne lydveje. Det, Claude Talk sender ind i den, kan VoiceMeeter sende videre til en mikrofon eller til dine høretelefoner, uden at Claude Talk kan se det. Det ville give hyl eller lade folk høre sig selv. Kun rene kabler (VB-CABLE A/B/C/D, Hi-Fi Cable) kan derfor bruges, og deres optageside kan aldrig vælges som mikrofon.

## K16 – Højttalere opdages på Windows' standardudgang
Ekkodæmpningen slås automatisk til, når Windows' standardudgang er højttalere. Spiller Discord eller Claude-vinduet på en anden enhed end standarden, og er den enhed højttalere, opdages det ikke. Slå så ekkodæmpning til under **Indstillinger**.

## K17 – Links til hjælp og opdateringer kræver et offentligt projekt
»Skriv til os« og hentelinket peger på projektets GitHub-side. Så længe projektet er privat, virker de kun for folk med adgang. Opdateringer er ikke slået til i denne udgave (der er ingen signeringsnøgle til opdateringer endnu, se `SIGNING.md`). Når projektet gøres offentligt, virker linkene uden ændringer i programmet.

## K18 – Hvis Claude Talk bliver dræbt, mens Discord optager
Lukker man Claude Talk normalt, spørger den først og giver Discord sin egen mikrofon tilbage. Bliver programmet dræbt (Jobliste → Afslut opgave, eller strømmen går), kan det ikke nå det. Discord optager så fra en tavs mikrofon, indtil Claude Talk startes igen, eller man vælger sin mikrofon i Discord. Ved næste start retter Claude Talk det selv.

## K19 – Mindre ting fra lydgennemgangen, der ikke er ændret
- Hver mixer regner med 10 ms-blokke til drift-regulering og efterslæbsgrænser. Enheder med andre perioder fungerer, men reguleringen er tunet til 10 ms.
- Broen til Claude-vinduet begrænser antallet af ventende pakker, ikke deres alder i Chromiums kø. Målt forsinkelse til Claude står i `PROGRESS.md`.
- Når en tilføjet kilde fjernes i **Avanceret**, stopper dens lyd uden udtoning (et lille klik er muligt).

## K20 – Windows på ARM (Snapdragon)
Installeren bruger VB-Audios ARM64-driver, hvis pakken har en. Har den ikke, siger installeren og programmet tydeligt, at Claude Talk-mikrofonen ikke findes til pc'en endnu (kode 6). Det er ikke testet på en ARM-pc.

## K21 – Discord i browseren og andre Discord-programmer
Claude Talk genkender Discord, Discord PTB og Discord Canary. Discord i en browser og programmer som Vesktop eller Legcord genkendes ikke. Kortet bliver ved med at sige »Start Discord«.

## K22 – Login med firma-konto (SSO)
Logger man ind på Claude via sin arbejdsplads (SSO), åbner login-siden i den almindelige browser, og Claude-vinduet bliver ikke logget ind. Brug login med e-mail og kode.

## K23 – Hvis Discords højttaler sættes til Claude Talks interne enhed
Claude Talk sætter Windows' standardhøjttaler tilbage, hvis den bliver »Claude Talk – intern« (D23). Vælger man den som højttaler inde i Discords egne indstillinger eller i Windows' lydstyrkemixer pr. program, opdages det ikke, og de andre hører sig selv.

## K24 – To Windows-brugere på samme pc
Kablet deles af alle brugere. Lader bruger A Claude tale, og skifter man til bruger B uden at logge A ud, kan B's Discord høre A's Claude.

## K25 – Forsinkelse til Claude
Vejen fra Discord til Claude-vinduet går gennem en buffer, broen til siden og sidens egen lydkø. Målt forsinkelse står i `PROGRESS.md`. Den er over målet på 40 ms. For en samtale med Claude betyder det, at Claude hører de andre en anelse senere; Claude svarer alligevel først, når nogen er færdig med at tale.

