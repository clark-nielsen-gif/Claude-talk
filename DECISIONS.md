# Beslutninger

## D1 – Virtuelle lyddrivere (bevist i CI, se `driver-proof.yml` og grenene `ci-results/*`)

### Vigtigt om CI-beviset
GitHubs Windows-runnere starter med **test-signing slået til** (det er billedets standard, og det kan ikke slås fra uden en genstart, som en hostet runner ikke overlever). At en driver installerer på runneren, beviser derfor intet. Produktionssignatur bevises i stedet ud fra signaturkæden på den pakke, Windows faktisk har lagt i DriverStore: `signtool verify /kp` (Windows' kernepolitik) skal bestå, og kataloget skal være signeret af Microsoft (attestation/WHQL).

### Kandidat A: VirtualDrivers "Virtual Audio Driver" 25.7.14 – **afvist**
- Signeret af "SignPath Foundation" (Authenticode). `signtool /kp` fejler. Ingen Microsoft-attestation, ingen WHQL. Kræver test-signing på en almindelig pc.
- INF'en er målrettet `NT$ARCH$.10.0...22000` (kun Windows 11). Én mikrofon. Navnet ligger i INF'en.

### Kandidat B1: VB-CABLE (gratis, `VBCABLE_Driver_Pack46.zip`) – **godkendt**
Bevist på `windows-latest`, `windows-2025` og `windows-2022`:
- Installeret pakke: `vbmmecable64_win10.inf`, version 3.5.2.0 (31-10-2026). Katalogsigner: **Microsoft Windows Hardware Compatibility Publisher**, attribut **Attested**. `signtool /kp` består. Det er en produktionssignatur.
- `VBCABLE_Setup_x64.exe -i -h` installerer lydløst **uden** driverdialog og uden at udgiveren er betroet på forhånd. Afslutningskode 0, enheder aktive uden genstart.
- Mikrofonen kan omdøbes via Core Audio-API'et (`IMMDevice::OpenPropertyStore(STGM_READWRITE)`), samme mekanisme som Windows' Lyd-kontrolpanel. Ingen registry-hacks og intet SYSTEM-trick nødvendigt. Navnet overlever en genstart af lydtjenesten.
- **Navnet bliver "Claude Talk – til Discord (VB-Audio)".** Windows sammensætter altid det synlige navn som "beskrivelse (grænseflade)". At skrive det sammensatte navn direkte afvises med `E_ACCESSDENIED` (bevist i CI), og en tom grænseflade giver "()". "(VB-Audio)" er pænt og opfylder samtidig VB-Audios licensbetingelse om, at brugeren kan se, at kablet er fra VB-Audio.
- Windows Server-billeder har lydtjenesten slået fra. Den startes i CI. På almindelige Windows-pc'er kører den altid.

### Kandidat B2: VB-Audio Hi-Fi Cable (gratis, 2014) – **afvist**
- Installerer `vbhfvaio64_win7.inf`, som er signeret med VB-Audios gamle certifikat fra 2014 (krydssigneret, ikke Microsoft-signeret). Windows viser altid "Windows-sikkerhed"-dialogen (`pnpui.dll,InstallSecurityPromptRunDllW`), og installeren hænger, til nogen klikker. Dokumenteret med processer og skærmbilleder.
- Opfylder ikke kravet "Microsoft-attestationssigneret eller WHQL".
- Microsoft fjerner desuden standardtilliden til krydssignerede kernedrivere fra og med april 2026-opdateringen (Windows 11 24H2/25H2/26H1, Server 2025). Kun en liste over udvalgte, kendte drivere undtages. Hi-Fi Cable kan derfor holde op med at virke uden varsel.

### Kandidat B3: VB-CABLE A+B / C+D – **kan ikke hentes automatisk**
Kan kun hentes via et personligt link efter en donation i VB-Audios webshop ("pay what you want"). Installeren kan derfor ikke hente dem selv fra vb-audio.com.

### Konsekvens
Der findes præcis **én** virtuel mikrofon, der både er gratis, produktionssigneret og kan hentes automatisk: VB-CABLE. Produktet kræver **to**, ellers bliver ekko-reglerne brudt. Se `KNOWN_ISSUES.md` K3 for mulighederne. Valget afventer brugeren, fordi alle veje koster enten penge eller en tung afhængighed.

## D2 – CI-runnere
- `windows-latest` (er p.t. Server 2025, build 26100), `windows-2025` og `windows-2022` (build 20348, nærmeste til Windows 10). Der findes ingen hostet Windows 10-klient.

## D3 – CI-resultater læses via git
Udviklingsmiljøet kan ikke hente GitHub Actions-artefakter (proxyen blokerer Azure-blob-lageret). Hvert job lægger derfor sine resultater (JSON, logs, skærmbilleder) på sin egen gren `ci-results/<run>-<forsøg>-<job>`, som hentes med `git fetch`. Grenene er kun bevismateriale og kan slettes uden konsekvens.

## D4 – Repository
`clark-nielsen-gif/Claude-talk` (privat, oprettet af brugeren). Bemærk: Private repositories har en begrænset kvote af gratis Actions-minutter, og Windows-minutter tæller dobbelt. 2-timers udholdenhedstesten bruger en mærkbar del af kvoten.
