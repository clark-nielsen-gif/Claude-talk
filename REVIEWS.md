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

## Runde 4 – 2026-11-26 – ikke ren

| Rolle | Resultat | Vigtigste fund |
|---|---|---|
| Kode | 1 major, 5 minor | En opgradering kørte hele afinstallations-oprydningen: kablet blev fjernet og hentet igen, og brugeren blev spurgt (med Ja som standard) om at slette login. |
| Lyd | 1 blocker, 4 minor | Efter ét hul i kilden kunne ekkodæmperens reference blive tavs for altid (»gælden« blev aldrig betalt). Timer-drosling i Windows 11, når programmet ligger ved uret. |
| Design | 2 major, 11 minor | Opsætningens problem-skærm kunne ikke rulle, så »Gå til oversigten« var uden for skærmen. Mikrofon-trinnet fik flueben, selv når mikrofonen var blokeret. |
| "Min mor" | 1 betinget blocker, 5 major, 18 minor | Hentelinket virker kun, når projektet er offentligt (K17). »Gendan« efterlod en vejledning, der aldrig kunne lykkes. Mikrofon-slukning overlevede en genstart. Login-hjælp kun ved Google. Discord i browseren nævnt ingen steder. |
| "Bryd det" | 6 major, 9 minor | To Claude-vinduer ved dobbeltklik. Luk Claude Talk, mens Discord er lukket, efterlod Discord på det tavse kabel. Opgradering (som kode-revieweren). Kablet kunne holde op med at virke, mens alt viste grønt. Smart App Control blokerer usignerede programmer. |

**Rettet efter runde 4:** alle blocker- og major-fund undtagen to, som kun kan løses uden for koden og er beskrevet i `KNOWN_ISSUES.md`: projektet skal gøres offentligt, før hentelinket virker for andre (K17), og programmet skal signeres for Smart App Control (K26). Opgraderingen er rettet efter Tauris installer-skabelon, men ikke kørt i CI (K27).

## Runde 5 – 2026-11-26 – ikke ren

| Rolle | Resultat | Vigtigste fund |
|---|---|---|
| Kode | 1 major, 4 minor | Gendan-hjælperen så den lukkende Claude Talk som »kører stadig« og stoppede med det samme; under en installer kunne den låse programfilen. |
| Lyd | 1 major, 4 minor | Ekkodæmperens reference fulgte fyldningen, ikke forsinkelsen: når mikrofonens og højttalerens ure er forskellige, faldt ekkodæmpningen i perioder til 1–18 dB. Jitterbufferen skrumpede aldrig efter vækst; mikrofonpauser fik den til at vokse. |
| Design | 1 major, 11 minor | Under 900 px lå problemkortet langt nede under det grønne »Dig«-kort. |
| "Min mor" | 2 major, 15 minor | Claude-vinduet kunne være højere end skærmen (stemme-knappen under proceslinjen). Login-hjælpen kom kun én gang, og programmet pegede på en stemme-knap, mens hun stadig var på login-siden. |
| "Bryd det" | 2 major, 6 minor | Samme hjælper-fejl. Discords »Tryk for at tale« giver kun Claude, mens tasten holdes, mens Claude Talk viser grønt. |

**Rettet efter runde 5:** hjælperen venter på, at Claude Talk er helt lukket, og startes kun ved brugerens egen lukning; installeren stopper en efterladt hjælper. Siden melder login-tilstand, og kortene siger »log ind«. Claude-vinduet passer til skærmen. Smalle vinduer viser problemkortet først. Discord-genstart tjekker Discords starter først. »Tryk for at tale« og forsinkelsen er beskrevet ærligt (K28, K29).

## Runde 6 – 2026-11-26 – ikke ren

| Rolle | Resultat | Vigtigste fund |
|---|---|---|
| Kode | 1 major, 3 minor | Den forhøjede installer kørte `taskkill` uden fuld sti (kunne starte en plantet fil fra Overførsler som administrator). |
| Lyd | 1 major, 3 minor | En mikrofon, der leverer flere 10 ms-rammer ad gangen, fik ekkodæmperens reference til at løbe tør (0–8 dB ekko fjernet). |
| Design | 2 major, 10 minor | Pladsholdertekst under 4,5:1 i mørkt tema. »Gør klar til Discord« fik flueben ved siden af en fejl. |
| "Min mor" | 1 major, 22 minor | Billederne i README var fra en tidlig version, og vejledningsbilledet sagde det modsatte af programmet. |
| "Bryd det" | 1 blocker, 1 major, 9 minor | Startet med Windows (uden hovedvindue) lukkede Claude Talk helt, når Claude-vinduet blev lukket. Højttalere tilføjet under Avanceret fik ikke ekkodæmpning. |

**Rettet efter runde 6:** programmet lukker ikke, når sidste vindue lukkes; `taskkill` er fjernet (Tauris egen lukning tager også hjælperen); referencen renderes for det øjeblik, mikrofonrammen blev optaget, og forsinkelsesvagten på faste routes regner med forsinkelsen (testen kører nu også med 2 og 3 rammer ad gangen: 64 dB); højttalere tilføjet under Avanceret slår ekkodæmpning til; COM i opstarten; et mislykket programskifte slår tilstanden helt fra; pladsholdere har 5,5:1; README-billederne er nye; lukke-dialogen fra ikonet ved uret kører uden at stoppe lyden.

## Runde 7 – 2026-11-26 – ikke ren (design og "min mor" rene)

| Rolle | Resultat | Vigtigste fund |
|---|---|---|
| Kode | 1 major, 4 minor | Efter et nedbrud i en WebView2-hjælpeproces (GPU, lyd-tjeneste, iframe) troede Claude Talk, at Claude ikke lyttede mere, og sendte ingen lyd, selv om samtalen kørte videre. |
| Lyd | 1 major, 2 minor | Stilhed, der blev fyldt ind for en kilde, blev aldrig afstemt med en pakke, der bare kom sent: referencens forsinkelse hoppede, og ekkodæmpningen faldt til 2–11 dB, når pakker af og til var over 5 ms forsinkede. |
| Design | 0 major, 9 minor | **Ren.** |
| "Min mor" | 0 major, 17 minor | **Ren.** |
| "Bryd det" | 2 major, 7 minor | Samme WebView2-fund. Discords gemte indstilling blev slettet ved hver nedlukning af Windows, så »genstart Discord« kom igen hver dag. |

**Rettet efter runde 7:** kun browser- og side-nedbrud nulstiller Claudes mikrofon, og siden bekræfter hvert andet sekund, at den bruger den; den del af en sen pakke, der allerede er spillet som stilhed, springes over (WASAPI's tidsstempel), så tidslinjen ikke vokser; Discords indstilling bevares ved Windows-nedlukning, når Claude Talk starter med Windows; »Claude stille i Discord« overlever ikke en genstart; instruktioner sendes ikke, mens Claude er ude af opkaldet; README forklarer »hver gang«.

## Runde 8 – 2026-11-26 – ikke ren (design ren)

| Rolle | Resultat | Vigtigste fund |
|---|---|---|
| Kode | 2 major, 3 minor | »Luk, og genstart Discord« gav ikke Discord sin mikrofon tilbage (runde 7-ændringen). Ved Windows-nedlukning blev Discords indstilling bevaret, selv om autostart var slået fra under Windows' startprogrammer. |
| Lyd | 1 major, 2 minor | Runde 7-rettelsen var ikke hel: stilheden blev regnet fra det øjeblik, en pakke blev *modtaget*, ikke fra hvor dens lyd sluttede, og en fast route betalte sin gæld tilbage mod målet i stedet for det niveau, reguleringen faktisk holder. Med 1 % sene pakker faldt ekkodæmpningen til 2 dB i revisorens simulering. |
| Design | 0 major, 10 minor | **Ren.** |
| "Min mor" | 1 major, 16 minor | Tauris egne installer-tekster findes ikke på dansk: »Claude Talk is running! Click OK to kill it« ved afinstallation, plus to spørgsmål om data. |
| "Bryd det" | 0 major, 5 minor | **Ren.** Begge fund fra første gennemgang (genstart af Discord, autostart slået fra i Windows) er rettet. |

**Rettet efter runde 8:** Discord får altid sin mikrofon tilbage, når brugeren selv lukker; om Claude Talk starter med Windows, læses i Windows (Run og StartupApproved); danske tekster til Tauris installer (`installer/Danish.nsh`); kun Tauris egen »Slet også indstillinger og Claude-login«-boks (ét spørgsmål); en fejl ved start af installeren vises som fejl, ikke som »annulleret«; indstillingerne hentes igen efter en afvist genvej; udholdenhedstesten måler CPU og RAM for hele proces-træet.

**Rettet efter runde 8 (lyd og "bryd det"):** stilheden fortsætter fra der, hvor den sidste pakkes lyd sluttede (WASAPI's QPC-tid); en fast route betaler gæld tilbage og starter efter en ny opfyldning ved det niveau, reguleringen holder (mål + en halv blok − kildens alder); lyd efter et overspringet stykke tones ind over 1 ms; ekkodæmperens tidsstempel læses, så en samtidig mikrofonpakke ikke tæller med. Ny test `late_packets.rs` (en model af optagesløjfen, ni scenarier: sene pakker, pauser, ±150 ppm, 3 ms levering, mikrofon i par): referencens forsinkelse holder sig inden for ±1,2 ms. I revisorens simulering fjernes 19–47 dB ekko igen (før 0,5–9,5 dB). Desuden: de sidste syv danske installer-tekster (WebView2), en pakke der er væk hos VB-Audio vises ikke som »ingen internet«, kun én luk-dialog ad gangen, indstillinger læses tilbage efter hver ændring, og højttaler-beskeden siger, at det er Windows' navn for enheden.

## Runde 9 – 2026-11-26 – ikke ren (alle fem roller havde fund)

| Rolle | Resultat | Vigtigste fund |
|---|---|---|
| Kode | 1 major, 10 minor | Efter »Gendan normale lydindstillinger« viste hvert Discord-opkald »Discord bruger en anden mikrofon« med en vejledning, der aldrig kunne lykkes. |
| Lyd | 1 major, 7 minor | En sen pakke, der helt var dækket af stilhed, flyttede stadig tidslinjen; `late_packets.rs` bestod kun med sin ene faste tidsfølge. Med andre tidsforløb hoppede referencen op til 10 ms. |
| Design | 1 major, 12 minor | Hjælp sagde »Lyden kører fint« med grønt flueben, mens Oversigt viste en rød fejl. |
| "Min mor" | 1 major, 12 minor | Vejledningen sagde »vælg Standard«. Havde hun valgt sin rigtige mikrofon i Discord, fik de andre nu Windows' standardmikrofon (webcam, bærbar), og intet i programmet kunne ændre det. |
| "Bryd det" | 1 major, 5 minor | Samme mikrofonfund, plus at Discords egen mikrofon under Windows' app-indstillinger blev ryddet i stedet for sat tilbage. |

**Rettet efter runde 9:**
- *Mikrofon (D28):* Claude Talk overtager den mikrofon, Discord brugte, før brugeren sættes til »Standard«; vejledningen og feltet Dig viser navnet; ny »Din mikrofon« under Indstillinger; Discords egen app-indstilling i Windows gemmes og sættes tilbage.
- *Lyd (D29):* én tidslinje for pakker og stilhed, også når en sen pakke er helt dækket; tabte pakker fyldes med stilhed; overgange til stilhed tones ud; 100 ms optagebuffer; faste routes holder deres niveau efter et spring; sekvenslås om mikrofonens tidsstempel; `late_packets.rs` kører seks tidsforløb af elleve scenarier (højst 0,9 ms udsving).
- *Gendan:* Discord-feltet siger »Discord bruger din egen mikrofon« (ingen advarsel); knappen spørger én gang til.
- *Hjælp:* viser »Der er et problem« og en knap til oversigten, når Oversigt viser en fejl; tallene hedder »Claude Talks egen forsinkelse« og »Manglende lyd (prøver)«.
- *Installer:* et kabel, der er slået fra, installeres ikke igen og mister ikke sit mærke (kode 7); Windows 10 får ingen driver og ingen autostart (kode 8); en ukendt pakke får alle filer undtagen driverfiler og installationsprogrammet fjernet, før det køres som administrator; status under installationen siger, at det kan tage et minut.
- *Robusthed:* Discords hjælpeprocesser tæller ikke som en genstart; en mislykket genstart af Discord siges højt; en indstillingsfil, der ikke kunne læses, bliver ikke overskrevet uden en kopi; to klik på »Tilføj« giver ikke to ens id'er; registreringsscanninger springer store værdier over i stedet for at stoppe; loggen skifter fil hver dag, har et loft og ligger i `%LOCALAPPDATA%`.
- *Genveje:* kræver Ctrl eller Windows-tasten (ikke Shift+bogstav, Alt+F4 eller AltGr); kun én genvej lytter ad gangen, med en Fortryd-knap.
- *Design:* hovedvinduet passer til skærmen ved 150–175 %; instruktionerne kan kopieres; kun linjen under kontakten læses op; kortet, der kræver noget, kommer først, også for tastatur og skærmlæser; Avanceret er oversat og forklarer låste felter for skærmlæsere; fejl i Avanceret vises; sletning af en forudindstilling kræver to tryk; trinene i opsætningen siger »venter« efter en fejl; »Tal dansk« følger sproget.
- *Tekster:* README om at lukke Claude-vinduet efter opkaldet, Fn-tasten, aktiveringsordet (»Næsten«), afinstallationsvinduet, pilen ved uret; D7, D8, D14, D26, D27 rettet til koden.
- *Kendte begrænsninger tilføjet:* K33 (programmer, der leverer sjældnere end hvert 10. ms), K34 (meget hurtige højttalere), K35 (Windows' ordlyd).

## Runde 10 – 2026-11-26 – ikke ren (alle fem roller havde fund)

| Rolle | Resultat | Vigtigste fund |
|---|---|---|
| Kode | 3 major, 8 minor | Hjælpeprocessen og afinstallationen kendte ikke den gemte app-indstilling for Discord og ryddede den. En mikrofon sat for Discord under Windows' app-indstillinger blev aldrig overtaget. »Din mikrofon« opdaterede ikke skærmen og kunne ikke sættes tilbage til standard. |
| Design | 1 major, 13 minor | Samme »Din mikrofon«-fund. |
| "Min mor" | 2 major, 10 minor | »Genstart Discord« blev foreslået midt i opkaldet, selv om Discord havde en bestemt mikrofon valgt (en genstart hjælper ikke). Efter Gendan, Luk og afinstallation fik Discord på »Standard« Windows' standardmikrofon i stedet for hendes headset. |
| "Bryd det" | 2 major, 6 minor | De samme to fund om at give Discords mikrofon tilbage. |
| Lyd | 2 major, 5 minor | Runde 9's tidslinje stolede på hver pakkes tidsstempel til 0,25 ms; svinger stemplerne ±0,7 ms, blev almindelig lyd skåret i stykker (1,5 % tabt) og referencen hoppede. Stilhed fyldt helt op til »nu« gjorde en pakke, der kom 5 ms for sent, til ca. 15 ms stilhed på alle veje (K32 var forkert). |

**Rettet efter runde 10:** hjælpeprocessen og afinstallationen læser indstillingerne; Discord får brugerens mikrofon som app-indstilling, når Claude Talk giver slip (også overtaget fra Discord); en tom gemt værdi overskriver ikke den kendte; en mikrofon sat for Discord i Windows overtages også; »Din mikrofon« opdateres med det samme, sender besked til resten af programmet, og »Windows' standard« er et valg, der respekteres; »Genstart Discord« foreslås kun, når Discord står på »Standard«; kablets optageside tjekkes også (en slået-fra »Claude Talk – til Discord« vises som fejl); kun Discords registreringsposter ryddes; afinstallationens sammenligning af installationsprogrammets navn tager ikke hensyn til store og små bogstaver; en indstillingsfil, der ikke kunne læses, overskrives aldrig; tilføjede kilder kan ikke være Claude Talks egne enheder; hjælperen venter op til 30 dage; `late_packets.rs` kræver højst 1 ms spring; CI-tjekket af mikrofonvalget kræver, at der vælges en mikrofon, når der findes en. UI: knapper, der fører til en anden side, flytter fokus derhen; Fortryd virker med tastaturet; Gendan siger »Er du sikker?« med en Nej-knap; mikrofonhjælpen passer efter Gendan; en mikrofon, der er taget ud, vises med navn og »(ikke tilsluttet)«; vejledningen siger »ikke Claude Talk – til Discord«; tekster og små layoutfejl.

**Rettet efter runde 10 (lyd):** pakke efter pakke følger tidslinjen prøveantallet og trækkes kun 5 % mod stemplerne; stempler sammenlignes kun efter indsat stilhed eller ved spring over 3 ms; stilhed fyldes kun op til 10 ms før nu (halverer tabet ved sene pakker og fjerner tabet ved hver ny lyd efter en pause); pakker uden tidsstempel følger prøveantallet. `late_packets.rs` er revisorens verificerede model med fire nye scenarier med svingende stempler; grænserne er sat efter det målte (spring højst 1,5 ms, udsving højst 4 ms) og står sådan i D29. K32 siger nu ærligt, at sene pakker giver korte huller i lyden på alle veje. Ikke rettet: to adskilte lydstrømme (tabsfri til de almindelige veje, tilpasset til referencen) – K32.

## Runde 11 – 2026-11-26 – ikke ren (alle fem roller havde fund)

| Rolle | Resultat | Vigtigste fund |
|---|---|---|
| Kode | 2 major, 4 minor | Runde 10's »kun Discords poster« fjernede intet på Windows 11 24H2 (posterne har hashede navne, set i CI-data). Den mikrofon, Claude Talk selv gav Discord, blev næste gang gemt som brugerens egen. |
| Design | 2 major, 12 minor | Hjælp sagde »Lyden kører fint«, når kablet var installeret men slået fra. »Din mikrofon er slået fra … Discord kan ikke høre dig« var forkert efter Gendan. |
| "Min mor" | 1 major, 10 minor | Et headset, der både er valgt i Discord og er Windows' standard, gav stadig »Genstart Discord« midt i opkaldet. |
| "Bryd det" | 2 major, 4 minor | Samme »egen mikrofon«-fund. Med autostart slået fra gik Discords mikrofon tabt ved nedlukning, når Discord var lukket. |
| Lyd | 1 major, 3 minor | Runde 10's stilhed op til »nu − 10 ms« fik referencens regulering til at læse forsinkelsen 10 ms for kort under pauser; efter en lang pause kom referencen op til 10 ms for sent, og ekkodæmpningen faldt fra ca. 44 til ca. 20 dB i flere sekunder. |

**Rettet efter runde 11:** værdien Claude Talk selv giver Discord gemmes for sig og læses aldrig som brugerens egen; ingen app-indstilling, når mikrofonen er Windows' standard; et nyt mikrofonvalg efter Gendan gives til Discord med det samme; kablet installeret af Claude Talk: alle poster, der peger på det, fjernes (24H2), et VB-CABLE brugeren havde: kun Discords (også genkendt på nøglens standardværdi); CI kræver nu, at posterne fjernes med Discord lukket; RunOnce starter hjælperen ved næste login; genstart-dialogen beder om at tjekke Discords inputenhed først og har en knap dertil; Hjælp og Oversigt bruger samme tjek af kablet; »slået fra«-teksten siger ærligt, at Discord stadig hører dig, når Discord bruger mikrofonen direkte, og Gendan slår ikke-virkende »slået fra« fra; opsætningen kræver et virkende kabel, når lyden kører; overskriften kender »slået fra« og »hold nede«; genvejs- og højttalerbeskeder fører til det rigtige sted; besked om instruktionerne vises på alle sider; tip om Discords støjfjernelse under Hjælp; ændringer af indstillinger skrives sikkert til disken; tilføjede kilder, der ikke kan tjekkes, afvises; Discord, der lige er startet mens Claude Talk lukker, gives sin mikrofon tilbage; tekster.

**Rettet efter runde 11 (lyd):** kilden fortæller routerne, hvor lyden når til (også under stilhed); almindelige routes vokser ikke af huller i en kilde, der fyldes med stilhed; routes starter med 21,5 ms (ingen udfald fra start i testen, der nu tæller fra start); en pakke uden tidsstempel efter stilhed sammenlignes som en sen pakke. `late_packets.rs` har lange pauser (8 og 20 s) og kræver, at forsinkelsen ligger inden for 1,5 ms af det stabile tilfælde; alle 102 forløb ligger på 14,2–14,6 ms.

## Runde 12 – 2026-11-26 – ikke ren

| Rolle | Resultat | Vigtigste fund |
|---|---|---|
| Kode | 3 major, 3 minor | Den »givne« værdi blev gemt, selv om Discord aldrig fik den. RunOnce virkede kun én gang (Windows sletter posten før kørslen). Gemte kilder under Avanceret forsvandt, hvis enheden ikke var sat i ved start, og deres routes blev slettet. |
| Design | 3 major, 10 minor | Linjen under kontakten sagde »hører Claude« før et opkald; med kontakten slået fra sagde den »Discord hører dig stadig«, selv om mikrofonen var blokeret; Oversigt kendte ikke »Claude i Chrome«-tilstanden. |
| "Min mor" | 1 major, 4 minor | Efter Gendan og tænd igen midt i et opkald fik hun »vælg Standard«-vejledningen, der ikke kunne lykkes, i stedet for »Genstart Discord«. |
| "Bryd det" | 2 major, 6 minor | Den tilbagegivne mikrofon forsvandt ved en Discord-opdatering (Windows husker pr. version); samme RunOnce-fund. |
| Lyd | **Ren** (0 major, 3 minor) | Almindelige routes voksede stadig ved hver pause (flaget kom for sent); referencen kunne sjældent flytte 10 ms efter en pause med en mikrofon, der leverer i par; et kort kapløb om »når-til«-tiden. |

**Rettet efter runde 12:** værdier, Claude Talk har givet Discord, huskes først, når de er skrevet, og som en liste; »Genstart Discord« foreslås også, når Discord optager fra en af dem; efter Gendan tjekkes hvert 10. sekund, at Discord har den rigtige værdi (også efter en opdatering); app-indstillingen springes kun over, når mikrofonen er Windows' standard for alle roller; hjælperen sætter RunOnce igen og fjerner den først, når Discord har fået sin mikrofon; afinstallationen skriver ikke i brugerens mapper; gemte kilder venter på deres enhed som før (kun nye kilder skal kunne tjekkes). UI: »Tændt. Gå ind i et opkald i Discord.« før et opkald; kontakten slået fra med problemer siger det i advarselsfarve; Discord-feltet siger »Discord kan ikke høre dig lige nu«, når mikrofonen ikke virker; »Claude i Chrome/Edge/Claude-appen« vises som Claude i opkaldet, med Gør Claude stille; genstart-dialogen har »Åbn Discords lydindstillinger« som hovedknap og forklaringen først; vejledningens sidste trin siger, hvad man gør, hvis der allerede står »Standard«; kort, der kræver noget, har en ramme; Hjælp regner højttalere uden ekkodæmpning som et problem; en valgt mikrofon, der er taget ud, siger hvad der bruges i stedet; opsætningen viser et slået-fra kabel; knaptekster siger, hvad de gør; README-rækker for kablet. K8 og K36 opdateret/tilføjet.

**Rettet efter runde 12 (lyd):** en route afgør først, om et udfald skal gøre bufferen større, når det er klart, om der kom ny lyd eller en pause; stilheds-flaget sættes, før stilheden når routerne; ny test `route_pauses.rs` (revisorens model af en almindelig route med pauser).

## Runde 13 – 2026-11-26 – ikke ren (lyd ren for anden runde i træk)

| Rolle | Resultat | Vigtigste fund |
|---|---|---|
| Kode | 1 major, 6 minor | Efter Gendan blev brugerens egen ændring af Discords input i Windows sat tilbage hvert 10. sekund. |
| Lyd | **Ren** (0 major, 4 minor) | Testen `route_pauses.rs` talte et artefakt (indekset, der går rundt hvert 21,8 s) som udfald; kort udfald tidligt i en session ved 100–200 ppm; en route spiller et par ms senere efter en pause. |
| Design | 1 major, 11 minor | Med kontakten slået fra og »slået fra«-knappen trykket sagde linjen »Din mikrofon er slået fra«, mens Discord hørte en anden mikrofon. |
| "Min mor" | 1 major, 8 minor | Claude talte i Windows' standardudgang, selv om hun hørte Discord i sit headset (eller en skærm uden lyd). |
| "Bryd det" | 1 major, 3 minor | Samme fund som koden: brugerens eget valg i Windows blev overskrevet; en gammel gemt værdi slog »Din mikrofon«. |

**Rettet efter runde 13:** efter Gendan sættes Discords værdi kun, når den peger på kablet, eller når en ny Discord (opdateret/genstartet) ikke har nogen; en værdi, brugeren selv vælger i Windows, bliver brugerens egen; et valg under »Din mikrofon« erstatter en ældre gemt værdi; ny besked »Claude taler et andet sted end Discord« med en knap, der lader Claude tale i Discords enhed (D30); hjælperen træder til side og fjerner RunOnce, hvis Claude Talk allerede kører, og regner kun et bekræftet resultat som færdigt; en værdi, Windows ikke lader os læse, er »ukendt«; listen over givne værdier skrives samlet; ventende vækst glemmes ved en genstart af kilden; `route_pauses.rs` tæller ikke indeks-artefaktet. UI: linjen under kontakten siger, når Discord hører en anden mikrofon; »Claude i Chrome/Edge« venter på lyd fra programmet og viser dets måler; »Claude lytter, men er ude af opkaldet«; »Slå ekkodæmpning til« slår den til; genstart-beskeden hedder »Discord har ikke fundet Claude Talk-mikrofonen endnu« og knappen »Vis mig hvordan«; vejledningen ruller ikke forbi sin overskrift; rækker i Indstillinger brydes på smalle skærme; README om tryk-for-at-tale, installationssiderne og afinstallation.
