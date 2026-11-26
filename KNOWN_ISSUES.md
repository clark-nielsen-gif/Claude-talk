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
Windows' indstilling for mikrofon pr. program kan først sættes, når Discord har brugt lyd, og en optagelse, der allerede kører, flytter ikke altid med. Claude Talk prøver igen hvert andet sekund og viser "Genstart Discord for mig", hvis Discord stadig optager fra en anden mikrofon. Indstillingen huskes derefter af Windows, også efter en genstart af pc'en, så længe Claude Talk starter med Windows. Lukker man selv Claude Talk, gives Discords egen mikrofon tilbage, og næste gang kan en genstart af Discord være nødvendig igen. Det samme kan ske efter en Discord-opdatering, fordi Windows husker indstillingen pr. Discord-version.

## K9 – VB-CABLE lægger sin egen forsinkelse til
Claude Talks egen del af vejen til Discord er målt til 20 ms (`audio-e2e`, `latency_engine_pipeline`, `engine_ms`) og er under målet på 40 ms. VB-CABLE lægger sin egen buffer oveni: på CI-runnerne målt mellem ca. 20 og 96 ms. Indstillingen ligger i `HKLM\SOFTWARE\VB-Audio\Cable\VBAudioCableWDM_Latency` (standard 7168 samples). Vi prøvede 2048, 1024 og 512 med genstart af driveren ([kørsel 45826851380](https://github.com/clark-nielsen-gif/Claude-talk/actions/runs/45826851380)): kablets forsinkelse var stadig 85–95 ms. Indstillingen ændrer altså ikke forsinkelsen mærkbart, og Claude Talk lader den være. Den samlede forsinkelse fra din mikrofon til Discord er derfor typisk 40–120 ms. Til samtale mærkes det som en kort pause, ikke som ekko.

## K10 – Ekkodæmperen kører i sin egen tråd og allokerer
WebRTC AEC3-biblioteket (`aec3`) allokerer pr. blok. Det kører derfor ikke i en enhedstråd, men i en egen tråd med »Pro Audio«-prioritet, der læser mikrofonen fra en låsefri ring. En forsinket blok kan i værste fald give et kort udfald i mikrofonen, men aldrig i de andre lydveje.

## K11 – Afinstallation via en anden brugers administratorkode
Hvis en almindelig bruger afinstallerer med en administrators kode, rydder afinstallationen administratorens brugerindstillinger (autostart og Discords mikrofonvalg), ikke brugerens. Brugerens autostart peger så på et program, der ikke findes, og Windows ignorerer den. Discords mikrofonvalg peger på en enhed, der er fjernet, og Discord falder tilbage til standardmikrofonen.

## K12 – »Gendan normale lydindstillinger«, mens Discord er lukket
Windows' lydtjeneste husker Discords mikrofonvalg i hukommelsen, så det kan først ændres, når Discord kører. Claude Talk sletter det gemte valg med det samme og retter det, så snart Discord starter. Hvis Discord allerede er begyndt at optage, skal Discord genstartes én gang. Det viser Claude Talk selv. Lukker man Claude Talk, mens Discord er lukket, starter Claude Talk en lille hjælper uden vindue. Den venter på, at Discord starter, giver Discord sin egen mikrofon tilbage og lukker sig selv (højst 12 timer, og den træder til side, hvis Claude Talk startes igen). Hjælperen startes kun, når brugeren selv lukker Claude Talk, ikke når Windows lukker ned eller en installer lukker programmet. Den er gennemgået i kode, men ikke kørt i CI.

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
- Når ekkodæmpningen slås til eller fra (fx når man skifter mellem høretelefoner og højttalere), kan der komme et lille klik og et hul på ca. 20 ms i mikrofonen.
- Hvis Claude Talks hovedtråd hænger i mere end ca. 40 ms, springes en bid af lyden til Claude over (et lille klik).

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

## K26 – Smart App Control og manglende signatur
Claude Talk er ikke signeret endnu (`SIGNING.md`). Windows viser derfor »Ukendt udgiver«, og pc'er med Smart App Control slået til blokerer installationen helt. Det løses kun ved at signere installeren og programmet.

## K27 – Opgradering er ikke testet automatisk
En nyere `ClaudeTalk-Setup.exe` oven på en ældre afinstallerer først den gamle. Afinstallationens oprydning (kablet fjernes, spørgsmålet om login) springes over, når den køres af en installer (den kører så fra programmappen), men det er kun kontrolleret mod Tauris installer-skabelon, ikke kørt i CI, fordi der endnu kun findes én version.

## K28 – Discords »Tryk for at tale« og selv-mute
Discord sender kun lyd fra sin mikrofon, mens man holder »Tryk for at tale«-tasten, og intet, når man har slået sig selv fra. Claude Talk-mikrofonen er Discords mikrofon, så det gælder også Claude. Claude Talk kan ikke se Discords indstillinger (de ligger sammen med Discords login), så oversigten viser i stedet et fast tip om at bruge »Stemmeaktivitet«.

## K29 – Forsinkelse fra mikrofonen til Discord
Målt i det rigtige program på Windows-runnere (`app-e2e`): i alt ca. 100 ms fra mikrofonen til Discords mikrofon uden ekkodæmpning og ca. 130 ms med. Målt alene (`audio-e2e`, `latency_engine_pipeline`) er Claude Talks egen del 20 ms, men i hele programmet måles den til ca. 60 ms (95 ms med ekkodæmpning); resten tilskrives VB-CABLE. De to målemetoder deler tiden forskelligt mellem Claude Talk og VB-CABLE, og årsagen er ikke fundet. Målet på 40 ms for Claude Talks egen del er derfor ikke dokumenteret opfyldt i hele programmet. `app-e2e` fejler ærligt på det punkt.

## K30 – Avancerede kilder og kontakterne
En kilde, man selv tilføjer under **Avanceret** (en ekstra mikrofon eller et program), følger ikke den store kontakt, mikrofon-slukning, tryk-for-at-tale eller »Claude hører kun mig«, og den får ikke ekkodæmpning. Det er lavet som en mixer: man bestemmer selv dens routes. Låsene mod ekko og sløjfer gælder stadig.

## K31 – Installation uden for Program Files og pc'er med kun ét USB-headset
Vælger man en anden installationsmappe end Program Files, kan knappen »Installer Claude Talk-mikrofonen« i programmet ikke bruges (kør installeren igen i stedet). Har pc'en kun ét USB-headset som højttaler, og tages det ud midt i et opkald, kan Windows vælge »Claude Talk – intern« som højttaler. Claude Talk har så ingen anden enhed at sætte tilbage, før headsettet sættes i igen.

## K32 – Sjældne sene pakker og enheder, der kalder sig højttalere
Kommer Claudes eller Discords lyd af og til over 15 ms for sent fra Windows, har ekkodæmperens reference et kort hul. Forsinkelsen flytter sig ikke længere (testen `late_packets.rs`), men i simuleringen fjernes så »kun« 19–47 dB ekko i stedet for 66 dB, og i værste 2 sekunder ned til ca. 11 dB. Det gælder kun med højttalere. Nogle USB-headsets og skærme med HDMI-lyd melder sig til Windows som »Højttalere«. Claude Talk tror så, at du bruger højttalere, viser en besked om det og slår ekkodæmpning til. Det koster ca. 10 ms ekstra forsinkelse, men giver ikke ekko.
