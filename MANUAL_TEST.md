# Manuel test i et rigtigt Discord-opkald

Tag en ven med i et Discord-opkald. Brug høretelefoner, medmindre du tester trin 9. Hvert trin har ét forventet resultat. Skriv ✓ eller ✗.

1. **Installér.** Kør `ClaudeTalk-Setup.exe`. Du ser højst én Windows-sikkerhedsdialog (UAC) og ingen driverdialog. Ingen genstart.
   - [ ] Installeret uden flere dialoger
2. **Første opstart.** Claude Talk åbner med "Tag Claude med i dine Discord-opkald". Tryk "Sæt det op for mig". Alle fire punkter får flueben, og du ender på "Klar!".
   - [ ] Tre skærme, ingen tekniske ord
3. **Log ind på Claude.** Tryk "Åbn Claude". Claude-vinduet åbner på claude.ai. Log ind med din e-mail og koden, som Claude sender dig.
   - [ ] Logget ind. (Hvis du trykker "Fortsæt med Google", vises en venlig vejledning om e-mail-login i stedet.)
4. **Luk og åbn Claude-vinduet igen.**
   - [ ] Du er stadig logget ind
5. **Start Discord-opkaldet.** I Claude Talk står der "Discord hører Claude ✓" på Discord-kortet. Hvis der står "Discord bruger en anden mikrofon", så tryk "Vis mig hvordan", og følg de tre trin. Fluebenet kommer af sig selv.
   - [ ] Grønt flueben uden at åbne Windows' lydindstillinger
6. **Tænd for Claudes stemme** i Claude-vinduet (stemme-knappen ved tekstfeltet). Claude-kortet viser "Claude hører dig og Discord ✓".
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
10. **Knapperne.** Tryk "Gør Claude stille" og derefter "Slå min mikrofon fra".
    - [ ] Din ven hører ikke Claude, men du gør. Derefter hører hverken din ven eller Claude dig.
    - [ ] Genvejstasterne Ctrl+Shift+F9 og Ctrl+Shift+F10 gør det samme
11. **Store kontakt fra.** Slå "Claude i Discord" fra.
    - [ ] Din ven hører dig, men ikke Claude. Claude hører hverken dig eller Discord.
12. **Genstart Discord** midt i opkaldet.
    - [ ] Efter genstart virker alt igen uden at røre Claude Talk
13. **Lang samtale.** Lad opkaldet køre i 30 minutter.
    - [ ] Ingen klik, udfald eller voksende forsinkelse
14. **Gendan.** Indstillinger → "Gendan normale lydindstillinger".
    - [ ] Discord bruger igen din almindelige mikrofon
15. **Afinstallér** via Windows' "Installerede apps".
    - [ ] Claude Talk, "Claude Talk – til Discord"-mikrofonen og autostart er væk. Discord bruger din almindelige mikrofon.

Hvis noget fejler: Hjælp → "Gem fejlrapport", og send filen fra skrivebordet.
