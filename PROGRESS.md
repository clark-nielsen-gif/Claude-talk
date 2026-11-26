# Fremdrift

## 2026-11-26 – M0: fundamentet (virtuelle mikrofoner)

**Bevist i CI**, alle jobs grønne: [driver-proof #45783794886](https://github.com/clark-nielsen-gif/Claude-talk/actions/runs/45783794886) (resultater på grenene `ci-results/45783794886-*`):
- VB-CABLE installeres lydløst og uden driverdialog. Den er Microsoft-attesteret (produktionssigneret), består `signtool /kp` og er aktiv uden genstart på `windows-latest`, `windows-2025` og `windows-2022`.
- Den virtuelle mikrofon omdøbes via Core Audio-API'et og ses af programmer som **"Claude Talk – til Discord (VB-Audio)"**, også efter en genstart af lydtjenesten.

**Afvist med bevis:**
- VirtualDrivers 25.7.14: kun SignPath-signatur, fejler kernepolitikken.
- VB-Audio Hi-Fi Cable: gammel krydssigneret driver, tvinger en Windows-sikkerhedsdialog frem, og Windows' tillid til den slags drivere fjernes fra april 2026.

**Blokeret:** Den anden virtuelle mikrofon. Se `KNOWN_ISSUES.md` K3. Ifølge opgaven bygges resten ikke, før fundamentet er bevist, så M1 venter på brugerens valg.

**Undervejs fundet og løst:** GitHub-runnere kører med test-signing slået til, så beviset er lavet om til at bygge på signaturkæden. Artefakter kan ikke hentes fra udviklingsmiljøet, så resultaterne publiceres som git-grene.
