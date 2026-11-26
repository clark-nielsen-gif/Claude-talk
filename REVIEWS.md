# Review-runder

Hver runde laves af fem nye, uafhængige reviewere (ingen kender de tidligere runder): **kode**, **lyd**, **design**, **"min mor"** (ikke-teknisk dansk bruger) og **"bryd det"** (prøver at få programmet til at fejle). Projektet er færdigt, når to runder i træk er rene, altså uden blocker- og major-fund.

## Runde 1 – 2026-11-26 – ikke ren

| Rolle | Resultat | Vigtigste fund |
|---|---|---|
| Kode | 3 major, 8 minor | Når Claude Talk lukkes fra bakken, peger Discord stadig på kablet (Discord bliver stum). Installerens tillidskæde er for svag (mappe i ProgramData kan plantes af almindelige brugere, hash-afvigelse accepteres, enhver signatur godtages). Opdaterings-signaturen passer ikke, når SignPath har gen-signeret installeren. |
| Lyd | 7 major, 8 minor | Stilhed fyldes først efter 30 ms, så jitterbufferne vokser, når en kilde tier. Render-bufferen holdes fuld (ca. 22 ms ekstra). Ekkodæmperens tråd har ingen MMCSS og allokerer. Referencen mangler kilder i "Claude i Chrome"-tilstand. "Stereo Mix" kan omgå låsene. Kommandoer kan gå tabt, når en udgang mangler længe. Claude-broen allokerer i sin lydtråd. |
| Design | 7 major, 8 minor | Kortene modsiger fejlbeskeder og den store kontakt. Kontrast: hvid tekst på knapperne (3,1:1), lille grå tekst, flueben i mørkt tema og kontakten i "fra". Deaktiveret knap ligner en aktiv. Dialogen forsvinder i mørkt tema. |
| "Min mor" | 8 major, 7 minor | Kort, der modsiger fejl. "Ingen ekko" (forkert dansk). "Du ville høre sig selv". "Send den til os" uden adresse. Tekniske tal i Hjælp. Ingen knap til Claude-vinduet. README-link til en GitHub-side og en skræmmende SmartScreen-forklaring. |
| "Bryd det" | 7 major, 8 minor | En anden VB-Audio-enhed (Hi-Fi Cable, brugerens eget VB-CABLE) kan blive omdøbt og overtaget. Et nyt standard-headset følges ikke. Mikrofonen er åben hele tiden (Bluetooth i håndfri-tilstand). Stereo Mix omgår låsene. Installer-tillid. Chrome-tilstand plus højttalere giver ekko. VoiceMeeter fejlklassificeres. |

**Rettet efter runde 1** (commit `ed5448b`, »Review 1 fixes«):
- Alle major-fund.
- Fra minor-fundene: NaN-værn, ordnede kommandokøer, korrekt låserækkefølge i kontakterne, bakkekontakten, forudindstillinger mod Chrome-tilstand, frisk tilstand i Claude-vinduet, F-taster, selvtest-vagt, WebView2-profil lokalt, rigtigt skrivebord, backup af nyere config og Windows 24H2's lager for app-mikrofoner.

**Ikke rettet, med begrundelse:**
- Ekkodæmperens allokering (K10).
- VB-CABLE's egen buffer (K9).
- Afinstallation med en andens administratorkode (K11).
- Ikke-transitiv selvhørings-kontrol: ingen nuværende vej rammer den, fordi loopback-indgange er udelukket.
- Glidende overgange ved underrun og tilbagefald til ScriptProcessor: små lydpletter ved fejl, ikke ved normal drift.

## Runde 2 – 2026-11-26 – ikke ren

| Rolle | Resultat | Vigtigste fund |
|---|---|---|
| Kode | major-fund | Claude-siden kunne få den rigtige mikrofon (tilladelse gives til claude.ai). Installerens reserve-vej stoler på enhver signatur og bruger miljøvariabler og certutil. Mærket skrives, før kablet findes. Den bærbare udgave kan bede om administrator. Selvtesten kan køre, mens Claude lytter. F12 kan ikke være en global genvej. |
| Lyd | 2 blocker, 3 major, 9 minor | Ekstra kabler (CABLE-A m.fl.) kan blive valgt som mikrofon og give hyl. Jitterbufferens skrumpe-vagt virker aldrig, så bufferen ender på 12 ms og falder ud hvert 1–2 minut (to timers simulering). Ekkodæmperens reference flytter sig med bufferen. Pauser og genstarter får bufferne til at vokse. |
| Design | major-fund | Kortenes tekster passede ikke til den rigtige tilstand. Kontrast på flere tekster. Rækkefølgen i menuen. |
| "Min mor" | major-fund | SmartScreen-hjælpen stod for langt nede. Manglende svar på »Claude svarer ikke« og »Jeg har lukket vinduet«. Uens ord (Installér/Installer). |
| "Bryd det" | 6 major, 9 minor | Instruktioner kunne overskrive et udkast eller havne i profilindstillinger. Vores kabel kunne blive Windows' standardhøjttaler. Ekstra kabler som mikrofon. Discord efterlades på et tavst kabel, når Claude Talk lukkes midt i et opkald. Et minimeret vindue holder mikrofonen åben. En anden Windows-brugers Discord kunne blive fanget. |

**Rettet efter runde 2** (commits `5479a02`, `106c5ae`):
- Alle blocker- og major-fund: se `DECISIONS.md` D11 og D23–D27.
- Fra minor-fundene: udtoning ved huller, genstarter vokser ikke bufferen, låste routes oprettes slukket, genvejsfejl vises, config fra nyere version reddes, egne instruktioner fjernes fra fejlrapporten, WebView2-nedbrud, afinstallation spørger om login, hurtigere opdagelse af opkald, selvtest-vagt.

**Ikke rettet, med begrundelse:** se `KNOWN_ISSUES.md` K15–K19 (VoiceMeeter, højttalere på en anden enhed end standarden, privat projekt, dræbt proces, små lydpunkter).

## Runde 3 – 2026-11-26 – ikke ren

| Rolle | Resultat | Vigtigste fund |
|---|---|---|
| Kode | ikke afsluttet | Revieweren ventede på en tilladelse til at hente en webside og blev stoppet. Runden var allerede ikke ren. |
| Lyd | 1 blocker, 2 major, 3 minor | Ekkodæmperens reference (10 ms) løb tør og flyttede sig: kun 4–27 dB ekko fjernet i en simulering af den rigtige kobling. Mikrofonens genstarter fik mikrofon-routes til at vokse med ekkodæmpning slået til. Forsinkelsen er ikke målt igen efter ændringerne. |
| Design | 1 blocker, 7 major, 14 minor | Opsætningen kunne snurre for evigt, hvis mikrofonen manglede. Linjen under kontakten sagde »alt virker«, mens kortene sagde andet. Beskeder forsvandt efter 2,6 s. Kontrast på tre steder. Mixeren overlappede i smalle vinduer. |
| "Min mor" | 1 blocker, 8 major, 16 minor | Samme opsætnings-blocker. Linket i login-mailen logger ind i den forkerte browser. »Genstart Discord« sagde ikke, at man ryger ud af opkaldet. Lukke-dialogen havde skæve mellemrum. Stemme-knappen var ikke beskrevet. Browserens og Windows' advarsler ved download og installation var ikke forklaret. |
| "Bryd det" | 1 blocker, 4 major, 12 minor | »Åbn Claude« lavede vinduet i en synkron kommando (Tauri dokumenterer en deadlock på Windows). En Discord med fast inputenhed gav en evig »genstart Discord«-løkke. Vejledningen fik brugeren til at vælge Claude Talk-mikrofonen fast i Discord. Den bærbare Claude Talk kunne tilføje sig selv som kilde. Windows på ARM. |

**Rettet efter runde 3:** alle blocker- og major-fund, se `DECISIONS.md` D21 og `KNOWN_ISSUES.md` K9 og K20–K25, samt de fleste minor-fund (knapper der stikker ud, tastaturfokus i dialogen, ensartede ord, installerbeskeder på ét sprog, første lukning forklarer ikonet ved uret, lyd-klik ved huller på blokgrænser).

