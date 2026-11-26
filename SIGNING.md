# Signering: sådan slipper du for advarsler og får automatiske opdateringer

Alt virker uden det her. De to ting nedenfor er valgfrie og kræver kun, at du indsætter nogle hemmeligheder i GitHub. Release-workflowet (`.github/workflows/release.yml`) opdager dem selv.

## 1. Automatiske opdateringer (gratis, 5 minutter)

Opdateringer hentes fra GitHub Releases og skal være signeret med din egen nøgle, så ingen andre kan sende en falsk opdatering.

1. Installér Node.js på en pc, og kør i en terminal:
   ```
   npx @tauri-apps/cli signer generate -w claude-talk-update.key
   ```
   Vælg en adgangskode (eller tryk Enter for ingen). Der oprettes to filer: `claude-talk-update.key` (privat – del den aldrig) og `claude-talk-update.key.pub` (offentlig).
2. På GitHub: **Settings → Secrets and variables → Actions → New repository secret**, og opret:
   - `TAURI_SIGNING_PRIVATE_KEY` = hele indholdet af `claude-talk-update.key`
   - `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` = adgangskoden (udelad, hvis du ikke valgte en)
   - `TAURI_UPDATER_PUBKEY` = hele indholdet af `claude-talk-update.key.pub`
3. **Gør repositoryet offentligt** (Settings → General → Change visibility). Programmet henter `latest.json` fra GitHub uden login, og det kan det kun fra et offentligt repository.
4. Lav en ny release (se nederst). Fra den version tjekker Claude Talk selv for opdateringer og viser "En ny version af Claude Talk er klar".

Gem den private nøgle et sikkert sted. Mister du den, kan eksisterende installationer ikke længere opdatere sig selv.

## 2. Kodesignering af installeren (fjerner SmartScreen-advarslen)

Uden signering viser Windows "Windows har beskyttet din pc", første gang nogen kører installeren. Med en signatur forsvinder den (efter at certifikatet har opbygget omdømme).

### Mulighed A: SignPath Foundation (gratis for open source)
Kræver et **offentligt** repository med en open source-licens (MIT er allerede valgt).

1. Ansøg på <https://signpath.org/apply> og angiv repositoryet. Når I er godkendt, opretter SignPath en organisation og et projekt til jer.
2. I SignPath: opret et projekt med slug `claude-talk`, en signing policy med slug `release-signing`, og forbind det med GitHub (SignPath's "GitHub trusted build system"). Kopiér din **API token** og dit **Organization ID**.
3. På GitHub: **Settings → Secrets and variables → Actions**:
   - Secret `SIGNPATH_API_TOKEN` = din API token
   - Under fanen **Variables**: `SIGNPATH_ORGANIZATION_ID`, `SIGNPATH_PROJECT_SLUG` = `claude-talk`, `SIGNPATH_POLICY_SLUG` = `release-signing`
4. Lav en ny release. Installeren sendes automatisk til SignPath og kommer signeret tilbage.

### Mulighed B: Azure Trusted Signing (betalt, ca. 10 USD/md.)
Følg Microsofts guide "Quickstart: Trusted Signing". Tilføj derefter et trin i `release.yml` med `azure/trusted-signing-action` efter "Collect files" på `dist-release/ClaudeTalk-Setup.exe`, med hemmelighederne `AZURE_TENANT_ID`, `AZURE_CLIENT_ID`, `AZURE_CLIENT_SECRET`, endpoint, kontonavn og certifikatprofil.

## Lav en release
```
git tag v1.0.1
git push origin v1.0.1
```
GitHub Actions bygger `ClaudeTalk-Setup.exe` og `ClaudeTalk-portable.zip` og lægger dem på Releases-siden.

## Hvorfor driveren ikke skal signeres af dig
Den virtuelle mikrofon er VB-Audios VB-CABLE. Den er allerede signeret af Microsoft (attesteret), og installeren henter den direkte fra vb-audio.com.
