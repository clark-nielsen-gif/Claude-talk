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

## Runde 14 – 2026-11-26 – ikke ren (lyd ren for tredje runde i træk)

| Rolle | Resultat | Vigtigste fund |
|---|---|---|
| Kode | 1 major, 6 minor | Runde 13's »træd til side« fik hjælperen til at træde til side for den Claude Talk, der lige havde startet den, og fjerne RunOnce. |
| Design | 2 major, 7 minor | Hjælp regnede ikke »Claude taler et andet sted« som et problem; med kontakten slået fra og mikrofonen slået fra sagde Discord-feltet »Discord hører kun dig«. |
| "Min mor" | 1 major, 8 minor | »Lad Claude også tale i …« flyttede i virkeligheden al pc'ens lyd, uden at sige det. |
| "Bryd det" | 1 major, 6 minor | Samme hjælper-fund. |
| Lyd | **Ren** (0 major, 4 minor) | Gammel lyd og hårde klip efter et længere stop i en udgang (sjældent, ved genstart af enheden); hyppige pauser holder routes lidt sent; en teoretisk evig løkke i ekkodæmperens tråd. |

**Rettet efter runde 14:** hjælperen venter først på den Claude Talk, der startede den, og træder kun til side med det samme, når den er startet fra RunOnce; den prøver igen, til Discord har sin mikrofon (op til 30 min pr. Discord-start), og regner et fjernet kabel som færdigt; en tom værdi regnes først som udfyldt, når det er bekræftet, og Gendan tæller som udfyldt; en værdi, Claude Talk gav, men som ikke længere er den rigtige, rettes; en værdi, »Din mikrofon« har erstattet, bliver aldrig brugerens egen igen; Claude Talks egne enheder bliver aldrig »brugerens egen«. »Spil al pc'ens lyd i …« siger, hvad den gør, sætter kun medier og konsol, og beskeden vises kun, mens Claude-vinduet er Claude i opkaldet; Hjælp regner den med. Discord-feltet med mikrofonen slået fra; »se herunder« når kablet mangler; kun kortet med årsagen får ramme; »ingen mikrofon« har en knap; vejledningens tegning kan læses og siger, at »Outputenhed« skal stå; knapper i beskeder brydes; »Afslut Discord«; README.

## Runde 15 – 2026-11-26 – ikke ren (lyd ren for fjerde runde i træk, kode og »min mor« rene)

| Rolle | Resultat | Vigtigste fund |
|---|---|---|
| Kode | **Ren** (0 major, 4 minor) | En værdi, »Din mikrofon« har erstattet, blev kun husket i hukommelsen; `stale_ours` kunne fortryde et bevidst valg i Windows; hjælperens fil kunne skrives af to processer på én gang. |
| Lyd | **Ren** (0 major, 3 minor) | Stilhedsfyld halter op til 10 ms efter; realtidstesten dækker ikke pauser; en kommentar. |
| Design | 1 major, 6 minor | Skift til/fra »Claude i Chrome/Edge« under Avanceret opdaterede ikke resten af vinduet, før det blev genindlæst. |
| "Min mor" | **Ren** (0 major, 7 minor) | Headsettet efter afinstallation; »vi« og »Claude Talk« blandet; trinnavne. |
| "Bryd det" | 1 major, 4 minor | Vælger man selv »Claude Talk – til Discord« i Discord, er man tavs, når Claude Talk lukker, uden at noget siger det; brugerens valg i Windows blev rettet tilbage hvert 10. s, hvis Claude Talk engang havde givet den værdi; hjælperen kunne regne det for færdigt, mens Discord stadig optog fra kablet; Windows' standardmikrofon kunne blive kablet. |

**Rettet efter runde 15:** Avanceret og opstarts-fallback sender den nye opsætning til hele vinduet; et valg af »Claude Talk – til Discord« i Discord vises efter Gendan med vejledning, og luk-dialogen, README og K37 siger det; kun den senest givne værdi (og dem, »Din mikrofon« har erstattet, nu også gemt i filen) rettes efter Gendan; hjælperen ser på Discords faktiske optagelse og beder om en genstart af Discord, hvis den stadig er på kablet (K38); Windows' standardmikrofon holdes væk fra kablet (D31); menuen ved uret siger »Claude hører ikke min mikrofon« efter Gendan; filen skrives med et midlertidigt navn pr. proces. UI: vejledningen sætter fokus på overskriften; Instruktioner siger, at Claude i Chrome/Edge selv sender; »ingen mikrofon« i opsætningen har en knap; ikonknapper brydes ikke; kontaktlinjen peger på beskeden om lyd herunder; Hjælp regner »Claude lytter, men er ude af opkaldet« med; README om headsettet efter afinstallation.

## Runde 16 – 2026-11-26 – ikke ren (kode, lyd og »min mor« rene)

| Rolle | Resultat | Vigtigste fund |
|---|---|---|
| Kode | **Ren** (0 major, 4 minor) | De værdier, »Din mikrofon« har erstattet, blev ikke skrevet til filen (en delvist anvendt rettelse); »senest givet« fulgte ikke med, når Claude Talk skrev »Standard« eller brugerens egen; hjælperens besked kom før RunOnce blev fjernet. |
| Lyd | **Ren** (0 major, 4 minor) | Ekkodæmperens ventesløjfe tjekkede ikke stop; to næsten uopnåelige stille grene uden fade. |
| Design | 1 major, 8 minor | Efter Gendan, med Discord stadig på kablet, slog »Slå min mikrofon fra« også Discord fra, mens tre steder sagde, at Discord stadig hørte en. |
| "Min mor" | **Ren** (0 major, 7 minor) | Intet README-svar på »De andre kan ikke høre mig«; »kig tilbage her« og »orange knap«; »rigtige højttalere«. |
| "Bryd det" | 1 major, 4 minor | Når standardenheden blev sat tilbage fra kablet, mens brugerens headset var taget ud, blev afløseren permanent; brugerens eget VB-CABLE som standard blev rettet hvert 2. s; panik-stien gav kun halvt slip. |

**Rettet efter runde 16:** efter Gendan slår »Slå min mikrofon fra« kun routen til Claude fra; afløser-enheden huskes pr. rolle, og brugerens enhed sættes tilbage, når den kommer igen (D31); et VB-CABLE, brugeren havde før, røres kun, mens Discord optager det; »Discord er sat til kablet« vises kun for en Discord, der ikke optog fra kablet, da Gendan blev trykket – ellers en rolig linje (K12/K37); erstattede værdier skrives i filen med »-«; »senest skrevet« følger alle skrivninger; panik-stien sætter hjælper og RunOnce som ved luk; RunOnce fjernes før hjælperens besked; ekkodæmperens ventesløjfe stopper ved stop. UI: kontaktlinjen peger ned på en Discord-advarsel også med kontakten slået fra; vejledningen efter Gendan lover ikke et grønt flueben og viser mikrofon-linket efter trinene; Indstillinger modsiger ikke Discord-feltet; Instruktioner i Chrome/Edge-tilstand; »åbn Claude Talk (ikonet ved uret)« i stedet for »kig tilbage her«; »højttalere i rummet«; ikoner står ved første linje i lange beskeder; README: »De andre kan ikke høre mig« og en egen linje om aldrig at vælge »Claude Talk – til Discord«.

## Runde 17 – 2026-11-27 – ikke ren (kode, lyd, design og »min mor« rene)

| Rolle | Resultat | Vigtigste fund |
|---|---|---|
| Kode | **Ren** (0 major, 6 minor) | Afløser-mikrofonen kunne være »Stereo Mix«; den gemte Discord-proces for K12 blev sat ved enhver tilbagegivelse og aldrig ryddet; kablets ejerskab blev ikke opdateret efter installation fra appen; panik-stien kørte også i de forhøjede tilstande. |
| Lyd | **Ren** (0 major, 3 minor) | Et ulige sekvenstal efter en panik midt i et push; en mute efter Gendan blev fuld mute, når Claude blev tændt igen; enheden holdes åben efter Gendan. |
| Design | **Ren** (0 major, 9 minor) | Discord-feltet kunne kortvarigt sige »din mikrofon er slået fra« efter Gendan; Indstillinger-tekster efter Gendan (Discord lukket, Gendan-knappen stadig aktiv). |
| "Min mor" | **Ren** (0 major, 6 minor) | Vejledningens knap uden for synsfeltet; »orange knap« i README; lange tekster. |
| "Bryd det" | 1 major, 7 minor | Afløseren blev stadig permanent efter en genstart, eller når kablet allerede var standard ved start; kapløb mellem Gendan og overvågningen; en »-«-linje alene i filen; overvågningstråden uden panik-beskyttelse. |

**Rettet efter runde 17:** afløsere gemmes i en fil og overlever en genstart; kendte Claude Talk ikke brugerens enhed, vælges den første passende, der sættes i senere; afløser-mikrofonen er aldrig et loopback-input; overvågningen fortryder routing, hvis Gendan eller en panik kom imellem; den gemte Discord-proces (K12) sættes kun ved Gendan og ryddes, når Discord ikke optager fra kablet; kablets ejerskab opdateres efter installation fra appen; panik-stien kører kun i appen; overvågningstråden startes igen efter en panik; filen tåler BOM og en tom »-«-linje; en mute efter Gendan ryddes, når Claude tændes igen; sekvenstallet gøres lige ved genstart af optagelsen. UI: Discord-feltet siger ikke »slået fra« efter Gendan; Indstillinger efter Gendan (mikrofon-tekst med Discord lukket eller på kablet, Gendan-knappen skjult, opstartsteksten); vejledningens knapper står fast nederst; Instruktioner i Chrome/Edge-tilstand gentager sig ikke; højttaler-beskeden; »Mens du er i opkaldet«; README nævner knappen ved navn. CI: listen over lydenheder efter afinstallation prøves i op til 40 s og genstarter lydtjenesten én gang.

## Runde 18 – 2026-11-27 – ikke ren (kode, lyd og »min mor« rene)

| Rolle | Resultat | Vigtigste fund |
|---|---|---|
| Kode | **Ren** (0 major, 4 minor) | Afløseren uden kendt mål skiftede til den næste enhed, der dukkede op; K12-mærket kun i hukommelsen; et mislykket filskift blev ikke logget. |
| Lyd | **Ren** (0 major, 4 minor) | Sekvenstallet efter en panik; kommandoer hober sig op i en udgang, der ikke spiller; afløser-mikrofonen kan afvige kortvarigt fra motorens valg. |
| Design | 1 major, 6 minor | Discord-vejledningen satte fokus og rulning tilbage ved hver statusopdatering (30 gange i sekundet), så tastaturbrugere ikke kunne nå knapperne. |
| "Min mor" | **Ren** (0 major, 7 minor) | »Nej« i luk-dialogen sagde ikke, hvordan man genstarter Discord; forkert besked, når Discord ikke kunne lukkes; »ikonet ved uret« var tvetydigt. |
| "Bryd det" | 1 major, 4 minor | En afløser uden kendt mål skiftede til næste enhed, der dukkede op (en skærm, der vågner), uden tidsgrænse og uden besked; afløseren kunne være en skærm eller »Steam Streaming«; en gemt afløser overstyrede et valg, brugeren havde levet med; filen overlevede afinstallation. |

**Rettet efter runde 18:** gætteriet med »næste enhed« er fjernet; kendte Claude Talk ikke brugerens enhed, bruges den, Windows selv sidst havde som standard (`Role:N`-tidspunkterne); afløseren vælges efter type, og »Steam Streaming« aldrig; beskeden nævner enheden, også for mikrofonen; en gemt afløser droppes, hvis brugerens enhed allerede er tilbage ved start; afinstallationen sletter filen; et mislykket filskift logges og rydder op. Vejledningen sættes kun op én gang (ny test); dens knapper har en kant øverst. Luk-dialogen siger, hvordan man genstarter Discord, og »Discords ikon ved uret«; »Discord kunne ikke lukkes« har sin egen besked; menuen ved uret siger »Åbn Claude«; Indstillinger efter Gendan kalder rækken »Normale lydindstillinger«; højttaler-beskeden; README peger på »Lad Claude tale igen«. K18/K27 nævner opgradering midt i et opkald; K37 at K12-mærket kun huskes, mens Claude Talk kører.

## Runde 19 – 2026-11-27 – ikke ren (lyd og design rene)

| Rolle | Resultat | Vigtigste fund |
|---|---|---|
| Kode | 1 major, 6 minor | En afløser, der blev droppet ved start, blev kun droppet i hukommelsen og kom igen ved en senere start. |
| Lyd | **Ren** (0 major, 4 minor) | Mikrofonskift sammenlignes på navn; et headsets opkaldsmikrofon kunne blive afløser; et mål fra Windows' historik blev ikke tjekket for egnethed. |
| Design | **Ren** (0 major, 9 minor) | Shift+Tab fra vejledningens overskrift gik ud af dialogen; beskeden ved tilbageskift nævnte kablet forkert; mikrofon-beskeden uden knap; to beskeder delte én plads. |
| "Min mor" | 0 major, 6 minor (sluttede med »NOT CLEAN« trods 0 major; regnet som ikke ren) | »Ja, gendan« forsvandt efter 6 s; advarsler kun i det skjulte vindue; »Vi har sat din lyd tilbage« forsvandt efter 30 s. |
| "Bryd det" | 1 major, 4 minor | Runde 18's »brugeren har levet med afløseren« gjorde afløseren permanent efter en nat i dockingstationen; et tomt stik kunne blive afløser; »virtual« udelukkede rigtige headsets; Bluetooth-opkaldsdelen kunne vælges. |

**Rettet efter runde 19:** en gemt afløser, der stadig er standard ved start, går tilbage til brugerens enhed, når den er der (som Windows selv ville); afløseren er den tilsluttede enhed, Windows senest havde som standard, før der vælges efter type; typerækkefølgen afhænger af rollen; mål fra Windows' historik springer slåede-fra og uegnede enheder over (navnet læses fra registreringsdatabasen); ordlisten for virtuelle enheder er præcis; tilbageskift giver ingen forkert besked; output og mikrofon har hver sin besked i 2 minutter, mikrofonen med »Vælg en anden mikrofon«, og kontaktlinjen peger på dem. Vejledningens fokusfælde dækker overskriften (ny test); »Ja, gendan« står i 20 s; »Discord kunne ikke lukkes« nævner »Quit Discord«; samme navn for højttalerikonet.

## Runde 20 – 2026-11-27 – **ren** (alle fem roller)

| Rolle | Resultat | Vigtigste fund |
|---|---|---|
| Kode | **Ren** (0 major, 5 minor) | Mikrofon-beskedens knap ændrer Claude Talks mikrofon, ikke Windows'; installationstestens nød-tjek kunne bestå, hvis enhedslisten selv fejlede; nogle virtuelle enheder slipper gennem navnefilteret. |
| Lyd | **Ren** (0 major, 3 minor) | Ekkodæmpning til/fra midt i tale giver et klik og ~20 ms hul (som K19); mikrofonskift sammenlignes på navn; højttalertjekket ser kun på konsol-standarden. |
| Design | **Ren** (0 major, 6 minor) | Mikrofon-beskeden og dens knap taler om forskellige mikrofoner; kontaktlinjen blev ravgul for en besked, der kun fortæller; Hjælp sagde »fint« samtidig. |
| "Min mor" | **Ren** (0 major, 8 minor) | »Vi har sat din lyd tilbage« lød som en fejl; genstart-vejledningen har en omvej; lange første-gangs-tekster. |
| "Bryd det" | **Ren** (0 major, 6 minor) | Et bevidst genvalg af afløseren, mens Claude Talk var lukket, overstyres; brugerens egen VoiceMeeter blev ikke sat tilbage; ingen beskyttelse, mens Claude Talk er lukket; filen lå i den roamende profil. |

**Rettet efter runde 20 (mindre ting):** mikrofon-beskeden siger »til Claude og Discord«; »sat tilbage«-beskederne gør ikke kontaktlinjen ravgul, men peger stadig ned (»Se herunder«); output-beskeden forklarer roligt, at det typisk sker, når et headset tages ud; brugerens oprindelige enhed sættes tilbage, også når den ikke selv kunne være afløser; id'er sammenlignes uden hensyn til store/små bogstaver; filen ligger i `%LOCALAPPDATA%` og tåler BOM; installationstestens nød-tjek fejler, hvis enhedslisten fejler. K39 og README beskriver standardenheden, mens Claude Talk er lukket; D31 nævner genvalg af afløseren som kendt begrænsning.

## Runde 21 – 2026-11-27 – ikke ren (kode, lyd, »min mor« og »bryd det« rene)

| Rolle | Resultat | Vigtigste fund |
|---|---|---|
| Kode | **Ren** (0 major, 4 minor) | Id'er kun delvist sammenlignet uden hensyn til store/små bogstaver; `save()` fjernede ikke BOM; output-beskedens løfte holder ikke altid. |
| Lyd | **Ren** (0 major, 3 minor) | Intet fade, når mikrofonen lukkes eller skiftes midt i tale, eller når et program slutter midt i lyd; hårdt trin ved underløb i meget korte blokke. |
| Design | 1 major, 7 minor | Med en besked herunder sagde kontaktlinjen »De andre i opkaldet hører Claude«, selv om Claude var gjort stille i Discord (fejlen var ældre, men ikke fundet før). |
| "Min mor" | **Ren** (0 major, 6 minor) | »Nej« i luk-dialogen lukker; beskeden og »Dig«-feltet nævnte forskellige mikrofoner; README om afinstallation. |
| "Bryd det" | **Ren** (0 major, 4 minor) | »Brugerens enhed« kunne være »Stereo Mix« eller »Steam Streaming«, hvis Windows kort havde valgt den; op til 2 s ekko, før vagten reagerer (K31-tilfældet); filerne skrives uden `sync_all`. |

**Rettet efter runde 21:** kontaktlinjen siger altid, hvem der hører hvad i opkaldet, og tilføjer »Se herunder.«, når der er en besked (ny test); beskederne om flyttet lyd og mikrofon er neutrale (ikke ravgule), har titlen »Vi har flyttet …« og lover kun »normalt«; mikrofon-beskeden har kun knappen og »Claude og Discord hører dig igennem«, når »Din mikrofon« følger Windows. »Brugerens enhed« huskes kun, når det er en rigtig enhed; alle id-sammenligninger i vagten ser bort fra store/små bogstaver; filen tåler BOM også ved skrivning; begge filer skrives igennem til disken før omdøbning. README: »start Claude Talk, så retter det det selv« og en enklere linje efter afinstallation.

## Runde 22 – 2026-11-27 – **ren** (alle fem roller)

| Rolle | Resultat | Vigtigste fund |
|---|---|---|
| Kode | **Ren** (0 major, 3 minor) | En afløser blev glemt, hvis en uægte enhed kortvarigt blev standard; en midlertidig fil blev liggende, hvis skrivningen fejlede. |
| Lyd | **Ren** (0 major, 4 minor) | Ekkodæmpningen genopbygges ved hver fjernelse/tilføjelse af en kilde (preset-skift, Chrome-tilstand), også når kilden ikke findes; hårde klip ved efterslæb i ekkotråden og latensvagten (K10/K19). |
| Design | **Ren** (0 major, 7 minor) | Standardlinjen mistede halvdelen af »hvem hører hvad«, når der var en besked; ikke alle beskeder fik »Se herunder.«; flere beskeder læses højt på én gang. |
| "Min mor" | **Ren** (0 major, 5 minor) | Menupunktet »Min mikrofon er slået fra« lyder som en alarm; »Den må kun Claude Talk bruge« lyder som en regel, hun har brudt; to næsten ens README-linjer efter afinstallation. |
| "Bryd det" | **Ren** (0 major, 7 minor) | Efter Gendan sættes Discords værdi igen ved hver Discord-genstart, hvis brugeren har ryddet den i Windows; en gemt `discord_own_input` overlever afinstallation uden sletning; mikrofon-fallback i opremsningsrækkefølge kunne tage »Steam Streaming«. |

**Rettet efter runde 22 (små, sikre rettelser):** en afløser venter, mens en uægte enhed kortvarigt er standard; en standard, der ikke er i listen endnu, ses på igen næste gang; en mislykket skrivning logges og fjerner den midlertidige fil; »Steam Streaming« kan aldrig blive mikrofonen; kontaktlinjen beholder hele sætningen og tilføjer »Se herunder.«; »Den er kun til Claude Talk«; »Luk Claude Talk« i anførselstegn i installeren; én README-linje efter afinstallation. De øvrige mindre fund er noteret her og ændrer ikke, hvad brugeren hører i almindelig brug.

## Runde 23 – 2026-11-27 – ikke ren (kode, lyd, design og »bryd det« rene)

| Rolle | Resultat | Vigtigste fund |
|---|---|---|
| Kode | **Ren** (0 major, 3 minor) | En afløser overlever nu enhver uægte standard; et id, der aldrig kommer i listen, springes over for altid; en valgt »Steam Streaming«-mikrofon vises som »ikke tilsluttet«. |
| Lyd | **Ren** (0 major, 5 minor) | »Når-til«-tiden offentliggøres i to trin; en udgang, der har været nede over 500 ms, spiller lidt gammel lyd (runde 14); forkert fejltekst for et ikke-mikrofon-input i Avanceret. |
| Design | **Ren** (0 major, 4 minor) | Genvejs-advarslen får ikke »Se herunder.«; »Den« er lidt tvetydig i mikrofon-beskeden. |
| "Min mor" | 1 major, 6 minor | »Slå min mikrofon fra« blev ved at gælde i senere opkald (også uden Claude, med vinduet skjult), indtil Claude Talk blev genstartet. |
| "Bryd det" | **Ren** (0 major, 4 minor) | Efter Gendan blev et bevidst A→B→A-valg i Windows rettet én gang; luk inden for 2 s efter at gå ind i et opkald sprang dialogen over; hjælperen giver op for en Discord uden for `%LOCALAPPDATA%`. |

**Rettet efter runde 23:** mikrofon-muten slås til igen 30 sekunder efter, at opkaldet i Discord er slut (D32, README); luk-dialogen spørger Windows direkte, om Discord optager fra kablet; et værdi, brugeren selv vælger i Windows, bliver »det senest skrevne«, så et senere genvalg af en ældre værdi også er brugerens.
