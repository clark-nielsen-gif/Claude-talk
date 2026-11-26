# Claude Talk

**Tag Claude med i dine Discord-opkald.** Dine venner i Discord hører Claude, og Claude hører både dig og dem. Ingen ekko, ingen indstillinger at forstå.

![Claude Talk](docs/screenshots/main-ok--light.png)

## Sådan kommer du i gang

**1. Download.** Hent `ClaudeTalk-Setup.exe` under [Releases](../../releases/latest).

**2. Installér.** Dobbeltklik på filen. Windows spørger én gang, om programmet må ændre din pc. Tryk **Ja**. Installationen henter selv Discords nye mikrofon. Der skal være internet i et øjeblik.

**3. Tryk på knappen.** Claude Talk åbner. Tryk **Sæt det op for mig**, og derefter **Åbn Claude**. Log ind på Claude, start et opkald i Discord, og tænd for Claudes stemme i Claude-vinduet.

| Første gang | Klar |
|---|---|
| ![Velkommen](docs/screenshots/onboarding-welcome--light.png) | ![Klar](docs/screenshots/onboarding-ready--light.png) |

### Hvis Windows siger "Windows har beskyttet din pc"
Det sker, fordi programmet er nyt og endnu ikke har en købt signatur. Sådan kommer du videre:
1. Tryk på **Flere oplysninger** (den lille tekst i den blå boks).
2. Tryk på knappen **Kør alligevel**, der dukker op nederst.

Advarslen forsvinder, når programmet bliver signeret (se `SIGNING.md`).

## Det skal du vide
- **Claude kører i sit eget vindue i Claude Talk.** Det er det, der gør, at Claude kan høre dig og Discord, uden at noget andet på din pc kommer med. Du logger ind én gang, og så husker vinduet det.
- **Log ind med din e-mail.** Google tillader ikke login i programmers egne vinduer. Skriv din e-mail, så sender Claude dig en kode.
- **Discord bruger Claude Talk som mikrofon.** Claude Talk sætter det selv op, og din stemme går stadig igennem som før. Lad derfor Claude Talk køre (den starter med Windows og ligger nede ved uret).
- **Brug gerne høretelefoner.** Med højttalere slår Claude Talk selv ekkodæmpning til, men høretelefoner giver den bedste lyd.
- Programmet kræver **Windows 11**.

## Spørgsmål og svar

**Hører Claude sig selv?** Nej. Claude får kun din mikrofon og Discord, aldrig sin egen stemme. Discord får din mikrofon og Claude, aldrig sig selv.

**Kan jeg gøre Claude stille i Discord uden at lukke noget?** Ja. Tryk **Gør Claude stille** på Claude-kortet, eller brug **Ctrl+Alt+D**. **Ctrl+Alt+M** slår din egen mikrofon fra og til.

**Kan jeg bruge Claude i Chrome eller Claude-appen i stedet?** Nej. Claude Talk bruger sit eget Claude-vindue, så Claude kun hører det, den skal, og kun én gratis virtuel mikrofon er nødvendig. Se `KNOWN_ISSUES.md` (K3).

**Hvad sker der med mine samtaler og min adgangskode?** Ingenting. Du logger ind direkte på Anthropics egen side. Claude Talk læser aldrig adgangskoder, koder eller cookies.

**Hvordan fjerner jeg det igen?** Afinstallér Claude Talk under Windows' "Installerede apps". Discord får sin almindelige mikrofon tilbage, og Discords Claude Talk-mikrofon fjernes.

## Fejlfinding

| Det ser du | Det gør du |
|---|---|
| "Discord bruger en anden mikrofon" | Tryk **Vis mig hvordan**, og vælg *Claude Talk – til Discord* som inputenhed i Discord. Fluebenet kommer af sig selv. |
| "Discord skal lige genstartes" | Tryk **Genstart Discord for mig**. |
| "Windows blokerer din mikrofon" | Tryk **Åbn mikrofonindstillinger**, og slå "Giv apps adgang til din mikrofon" til. |
| "Vi kan ikke finde en mikrofon" | Sæt dit headset i. Claude Talk finder det selv. |
| "Tænd for Claudes stemme i Claude-vinduet" | Tryk på stemme-knappen ved tekstfeltet i Claude-vinduet. |
| Claudes stemme er svag i Discord | Slå Discords støjfjernelse fra under *Stemme og video*. |
| Noget andet driller | **Hjælp → Kør selvtest**. Hjælper det ikke, så **Hjælp → Gem fejlrapport**, og send filen fra skrivebordet. |

![Vejledning til Discord](docs/screenshots/main-discord-wrong-mic--light.png)

## Om
Claude Talk er et uafhængigt open source-projekt (MIT) og er ikke lavet af Anthropic eller Discord. Discords mikrofon er det virtuelle lydkabel **VB-CABLE fra VB-Audio** (donationware), som installeren henter direkte fra vb-audio.com. Støt gerne VB-Audio, hvis du bruger det.

Teknisk dokumentation: `PLAN.md`, `DECISIONS.md`, `KNOWN_ISSUES.md`, `PROGRESS.md`, `SIGNING.md`, `MANUAL_TEST.md`.
