# Manuel test i et rigtigt Discord-opkald

Tag en ven med i et Discord-opkald. Brug høretelefoner, medmindre du tester trin 9. Hvert trin har ét forventet resultat. Skriv ✓ eller ✗.

1. **Installer.** Kør `ClaudeTalk-Setup.exe`. Du ser højst én Windows-sikkerhedsdialog (UAC) og ingen driverdialog, og alle tekster i installeren er på dansk. Ingen genstart.
   - [ ] Installeret uden flere dialoger
2. **Første opstart.** Claude Talk åbner med »Tag Claude med i dine Discord-opkald«. Tryk »Sæt det op for mig«. Alle fire punkter får flueben, og du ender på »Klar!«.
   - [ ] Tre skærme, ingen tekniske ord
3. **Log ind på Claude.** Tryk »Åbn Claude«. Claude-vinduet åbner på claude.ai. Log ind med din e-mail og koden, som Claude sender dig.
   - [ ] På login-siden vises en dansk vejledning om login med e-mail og kode
   - [ ] Logget ind med koden fra mailen. (Hvis du trykker »Continue with Google«, vises vejledningen igen i stedet.)
4. **Luk og åbn Claude-vinduet igen.**
   - [ ] Du er stadig logget ind
5. **Start Discord-opkaldet.** I Claude Talk står der »Klar – venter på Claude« i Discord-feltet, og »De andre i Discord hører Claude ✓«, når Claudes stemme er tændt (trin 6). Hvis der står »Discord bruger en anden mikrofon«, så tryk »Vis mig hvordan«, og følg de tre trin. Fluebenet kommer af sig selv. Står Discord på »Standard« og var startet før Claude Talk, står der i stedet »Discord har ikke fundet Claude Talk-mikrofonen endnu«; har du valgt en bestemt mikrofon i Discord, kommer den besked ikke (kun vejledningen) – medmindre den mikrofon også er Windows' standardmikrofon (se 17b).
   - [ ] Grønt flueben uden at åbne Windows' lydindstillinger
6. **Tænd for Claudes stemme** i Claude-vinduet (stemme-knappen ved tekstfeltet). Claude-kortet viser »Claude hører dig og Discord ✓«.
   - [ ] Flueben
7. **Tal med Claude**, mens din ven lytter.
   - [ ] Din ven hører både dig og Claude tydeligt
   - [ ] Du hører Claude og din ven som normalt
   - [ ] Claude svarer på det, du siger
8. **Lad din ven stille Claude et spørgsmål.**
   - [ ] Claude svarer på din vens spørgsmål (Claude hører Discord)
   - [ ] Din ven hører *ikke* sig selv (intet ekko)
   - [ ] Claude reagerer ikke på sin egen stemme (Claude hører ikke sig selv)
9. **Højttalere** (valgfrit). Skift til pc-højttalere. Der kommer en besked om højttalere, og ekkodæmpningen slås til.
   - [ ] Din ven hører ikke Claude to gange, og Claude hører ikke sig selv
   - [ ] Også med bærbarens egne højttalere og mikrofon (K34)
9b. **Headset kun i Discord.** Vælg dit headset som *udgang* i Discord, mens Windows' standard er højttalerne. Der kommer en besked »Claude taler et andet sted end Discord«; tryk på knappen.
   - [ ] Bagefter hører du Claude i headsettet, og beskeden er væk
10. **Knapperne.** Tryk »Gør Claude stille« og derefter »Slå min mikrofon fra«.
    - [ ] Din ven hører ikke Claude, men du gør. Derefter hører hverken din ven eller Claude dig.
    - [ ] Genvejstasterne Ctrl+Shift+F9 og Ctrl+Shift+F10 gør det samme
11. **Store kontakt fra.** Slå »Claude i Discord« fra.
    - [ ] Din ven hører dig, men ikke Claude. Claude hører hverken dig eller Discord.
12. **Genstart Discord** midt i opkaldet.
    - [ ] Efter genstart virker alt igen uden at røre Claude Talk
13. **Instruktioner.** Gå til Instruktioner, og slå »Svar kun, når nogen siger et bestemt ord« til. Start en ny samtale i Claude-vinduet.
    - [ ] Instruktionerne står som første besked i samtalen og er sendt af sig selv
    - [ ] Claude svarer ikke, når I snakker sammen uden at sige »Claude«, men svarer, når nogen siger »Claude«
    - [ ] Med »Claude hører de andre i opkaldet« slået fra reagerer Claude ikke på din vens spørgsmål
13b. **Discords »Tryk for at tale«.** Slå det til i Discord, og lad Claude tale uden at holde tasten.
    - [ ] Din ven hører ikke Claude (som forventet). Oversigten viser tippet om »Stemmeaktivitet«. Slå »Stemmeaktivitet« til igen.
14. **Lang samtale.** Lad opkaldet køre i 30 minutter.
    - [ ] Ingen klik, udfald eller voksende forsinkelse
15. **Forkert højttaler.** Vælg »Claude Talk – intern« som højttaler i lydmenuen ved uret.
    - [ ] Inden for få sekunder spiller lyden igen i dine høretelefoner, og startsiden viser »Vi har sat din lyd tilbage«
16. **Luk midt i opkaldet.** Højreklik på ikonet ved uret, og vælg »Luk Claude Talk«.
    - [ ] Claude Talk spørger, om Discord skal genstartes. Svar **Ja**: Discord starter igen, og din ven kan høre dig. Start Claude Talk igen bagefter.
17. **Gendan.** Indstillinger → »Gendan normale lydindstillinger« (tryk to gange: »Ja, gendan«).
    - [ ] Discord bruger igen din almindelige mikrofon
    - [ ] Feltet Discord siger »Discord bruger din egen mikrofon« uden advarsel, også i et opkald
17b. **Mikrofon valgt i Discord.** Før installationen: vælg en bestemt mikrofon i Discord (ikke »Standard«), som ikke er Windows' standardmikrofon. Installer Claude Talk, og følg vejledningen. Gentag med et headset, der både er valgt i Discord og er Windows' standard: står der »Discord har ikke fundet Claude Talk-mikrofonen endnu«, skal dialogen bede dig tjekke Discords inputenhed først.
    - [ ] Vejledningen og feltet Dig viser den mikrofon, du valgte i Discord, og din ven hører den mikrofon
    - [ ] Der står ikke »Discord har ikke fundet Claude Talk-mikrofonen endnu« (en genstart ville ikke hjælpe)
    - [ ] Under Indstillinger → Din mikrofon kan du vælge en anden og gå tilbage til »Windows' standard«
    - [ ] Efter »Gendan« og efter »Luk Claude Talk« optager Discord igen fra den mikrofon, du valgte i Discord (selv om Discord nu står på »Standard«)
    - [ ] Efter »Gendan«: tænd »Claude i Discord« midt i et opkald. Feltet Discord viser »Discord har ikke fundet Claude Talk-mikrofonen endnu« (ikke en vejledning, der ikke kan lykkes), og efter genstarten hører din ven Claude
18. **Afinstaller** via Windows' »Installerede apps«.
    - [ ] Alle tekster i afinstallationen er på dansk. Sæt flueben ved »Slet også indstillinger og Claude-login«.
    - [ ] Claude Talk, »Claude Talk – til Discord«-mikrofonen og autostart er væk. Discord bruger din almindelige mikrofon. Åbner du Claude Talk igen efter en ny installation, skal du logge ind på Claude igen.

Hvis noget fejler: Hjælp → »Gem fejlrapport«, og send filen fra skrivebordet.

## D32 – Opkald, der starter med mikrofonen slået fra
1. Slå din mikrofon fra i Claude Talk, og luk vinduet med X. Gør Discord til fuld skærm (maksimeret).
2. Gå ind i et opkald i Discord. **Forventet:** inden for et par sekunder ligger Claude Talk-vinduet øverst på Oversigt med »Din mikrofon er slået fra«, og knappen på proceslinjen blinker. Discord har stadig tastaturet: skriv i Discords chat – intet sker i Claude Talk.
3. Tryk »Slå min mikrofon til«. **Forventet:** beskeden forsvinder, og de andre hører dig.
4. Gentag med vinduet åbent på Indstillinger, med vinduet minimeret (kun knappen på proceslinjen blinker), med vinduet minimeret og derefter lukket fra proceslinjen (knappen kommer tilbage og blinker), og med et spil i fuld skærm (kun knappen blinker; spillet beholder musen).
