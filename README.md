# Claude Talk

**Tag Claude med i dine Discord-opkald.** De andre i opkaldet hører Claude, og Claude hører både dig og dem. Intet ekko, ingen indstillinger at forstå.

![Claude Talk](docs/screenshots/main-ok--light.png)

## Sådan kommer du i gang

Du skal bruge Windows 11 og en Claude-konto (opret den gratis på claude.ai).

**1. Hent programmet.** Tryk her: [Hent Claude Talk](../../releases/latest/download/ClaudeTalk-Setup.exe). Filen lander i din Overførsler-mappe.

**2. Installer.** Dobbeltklik på filen. Windows spørger én gang, om programmet må ændre din pc. Tryk **Ja**. Installationen henter selv Claude Talk-mikrofonen, så der skal være internet i et øjeblik.

**3. Tryk på knappen.** Claude Talk åbner. Tryk **Sæt det op for mig**, og derefter **Åbn Claude**. Log ind på Claude, start et opkald i Discord, og tænd for Claudes stemme i Claude-vinduet.

| Første gang | Klar |
|---|---|
| ![Velkommen](docs/screenshots/onboarding-welcome--light.png) | ![Klar](docs/screenshots/onboarding-ready--light.png) |

### Hvis Windows siger »Windows har beskyttet din pc«
Det sker, fordi programmet er nyt, og Windows ikke kender det endnu. Det er sikkert at fortsætte:
1. Tryk på **Flere oplysninger** (den lille tekst i den blå boks).
2. Tryk på knappen **Kør alligevel**, der dukker op nederst.

## Det skal du vide
- **Claude kører i sit eget vindue i Claude Talk.** Det er det, der gør, at Claude kan høre dig og Discord, uden at noget andet på din pc kommer med. Du logger ind én gang, og så husker vinduet det.
- **Log ind med din e-mail.** Google tillader ikke login i programmers egne vinduer. Skriv din e-mail, så sender Claude dig en kode.
- **Discord bruger Claude Talk-mikrofonen.** Claude Talk sætter det selv op, og din stemme går stadig igennem som før. Lad derfor Claude Talk køre (den starter med Windows og ligger nede ved uret). Lukker du Claude Talk, får Discord din almindelige mikrofon tilbage.
- **Brug gerne høretelefoner.** Med højttalere slår Claude Talk selv ekkodæmpning til, men høretelefoner giver den bedste lyd.
- Programmet kræver **Windows 11**.

## Spørgsmål og svar

**Hører Claude sig selv?** Nej. Claude får kun din mikrofon og Discord, aldrig sin egen stemme. Discord får din mikrofon og Claude, aldrig sig selv.

**Kan jeg gøre Claude stille i Discord uden at lukke noget?** Ja. Tryk **Gør Claude stille** på Claude-kortet, eller brug **Ctrl+Shift+F9**. **Ctrl+Shift+F10** slår din egen mikrofon fra og til.

**Kan jeg bruge Claude i Chrome eller Claude-appen i stedet?** Som udgangspunkt nej: Claude Talk bruger sit eget Claude-vindue, så Claude kun hører det, den skal. Har du selv installeret et ekstra virtuelt lydkabel (VB-CABLE A eller B), kan det slås til under **Avanceret**.

**Hvad sker der med mine samtaler og min adgangskode?** Ingenting. Du logger ind direkte på Anthropics egen side. Claude Talk læser aldrig adgangskoder, koder eller cookies.

**Kan Claude kun svare, når nogen siger »Claude«?** Ja. Gå til **Instruktioner**, og slå »Svar kun, når nogen siger aktiveringsordet« til. Du kan vælge et andet ord, bede Claude svare kort, tale dansk eller skrive dine egne instruktioner, f.eks. »Du er spilleder i vores rollespil«. Claude Talk sender dem til Claude, hver gang du starter en ny samtale. Vil du have, at Claude slet ikke hører de andre i opkaldet, så slå »Claude hører de andre i opkaldet« fra.

**Mine venner kan ikke høre Claude.** Se på Discord-kortet i Claude Talk. Står der et grønt flueben ved »Discord hører Claude«, så tjek, at Claudes stemme er slået til i Claude-vinduet. Ellers følg knappen på kortet.

**Hvordan fjerner jeg det igen?** Afinstaller Claude Talk under Windows' »Installerede apps«. Discord får sin almindelige mikrofon tilbage, og Claude Talk-mikrofonen fjernes.

## Fejlfinding

| Det ser du | Det gør du |
|---|---|
| »Discord bruger en anden mikrofon« | Tryk **Vis mig hvordan**, og vælg *Claude Talk – til Discord* som inputenhed i Discord. Fluebenet kommer af sig selv. |
| »Discord skal lige genstartes« | Tryk **Genstart Discord for mig**. |
| »Windows blokerer din mikrofon« | Tryk **Åbn mikrofonindstillinger**, og slå »Adgang til mikrofon« og »Giv skrivebordsapps adgang til din mikrofon« til. |
| »Vi kan ikke finde en mikrofon« | Sæt dit headset i. Claude Talk finder det selv. |
| »Tryk på stemme-knappen …« | Tryk **Vis Claude-vinduet**, og tryk på stemme-knappen ved tekstfeltet. |
| Claudes stemme er svag i Discord | Slå Discords støjfjernelse fra under *Stemme og video*. |
| Noget andet driller | **Hjælp → Kør selvtest** (når du ikke er i et opkald). Hjælper det ikke, så **Hjælp → Gem fejlrapport** og **Skriv til os**. |

![Vejledning til Discord](docs/screenshots/main-discord-wrong-mic--light.png)

## Om
Claude Talk er et uafhængigt open source-projekt (MIT) og er ikke lavet af Anthropic eller Discord.

<sub>Claude Talk-mikrofonen er VB-CABLE fra VB-Audio, som installeren henter direkte fra vb-audio.com. Støt gerne VB-Audio, hvis du bruger den.</sub>

Teknisk dokumentation: `PLAN.md`, `DECISIONS.md`, `KNOWN_ISSUES.md`, `PROGRESS.md`, `SIGNING.md`, `MANUAL_TEST.md`.
