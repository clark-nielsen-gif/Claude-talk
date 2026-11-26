# Beslutninger

## D1 – Virtuel lyddriver (foreløbig, afventer CI-bevis i `driver-proof.yml`)

### Kandidat A: VirtualDrivers "Virtual Audio Driver" – fravalgt på forhånd, verificeres i CI
Undersøgt i kildekoden (`Source/Main/VirtualAudioDriver.inx`, `.github/workflows/VAD-compile-and-sign.yml`), ikke kun README:
- **Signatur:** Den "signerede" release 25.7.14 er signeret med et Authenticode-certifikat fra SignPath Foundation. Workflowet indsender ikke pakken til Microsoft (ingen attestation, ingen WHQL, ingen inf2cat/CAB-indsendelse). Siden Windows 10 1607 kræver kernedrivere med Secure Boot en Microsoft-signatur, så et SignPath-certifikat alene forventes at give fejlkode 52 uden test-signing. Projektets README kræver selv `bcdedit /set testsigning on` og kalder driveren beta.
- **Windows-versioner:** INF'en er målrettet `NT$ARCH$.10.0...22000`, altså kun Windows 11. Den installerer ikke på Windows 10.
- **To mikrofoner:** Driveren har én capture-endpoint ("Virtual Mic Driver by MTT"). To instanser kan forsøges som to root-enheder med samme hardware-ID (`ROOT\VirtualAudioDriver`). Det testes i CI.
- **Omdøbning:** Navnet ligger i INF'en. Ændres INF'en, brydes signaturen, så kun endpoint-omdøbning via property store er mulig.
- **Konklusion (foreløbig):** Opfylder ikke kravet om produktionssignatur. CI-jobbet `A` dokumenterer det med signtool `/kp` og enhedernes fejlkoder.

### Kandidat B: Officielle VB-Audio-kabler – valgt (foreløbig)
- **To gratis enheder uden betaling:** VB-CABLE A+B og C+D er kun til download efter donation via et personligt link, så de kan ikke hentes automatisk. Derfor bruges de to pakker, der er frit tilgængelige på vb-audio.com: **VB-CABLE** (`VBCABLE_Driver_Pack46.zip`, Win XP–11, x64/ARM64) og **Hi-Fi Cable** (`HiFiCableAsioBridgeSetup_v1007.zip`, 2014).
- **Licens:** VB-Audios licensside tillader at distribuere VB-CABLE med en app, så længe brugeren kan se og identificere VB-CABLE som en VB-Audio-app og har mulighed for at donere. Installeren henter alligevel filerne fra vb-audio.com (som opgaven kræver) i stedet for at bundle dem. For at overholde betingelsen viser installeren og "Om"-siden tydeligt "Virtuelle lydkabler fra VB-Audio" med link til donation, selvom selve enhedsnavnene omdøbes.
- **Én driverdialog:** VB-Audios egen forklaring er, at driverdialogen ikke kan undgås med `-i -h`. Installeren lægger derfor VB-Audios udgivercertifikat (taget fra pakkens .cat-signatur) i `TrustedPublisher` i det samme UAC-forløb. Så viser Windows ingen dialog for de to drivere. Certifikatet fjernes igen ved afinstallation, hvis Claude Talk selv lagde det der.
- **Risici, der skal bevises i CI:** Hi-Fi Cable er fra 2014 (krydssigneret med et certifikat fra før 29. juli 2015, som Windows stadig accepterer), og dens lydløse flag skal verificeres. VB-Audio anbefaler genstart efter installation. Det måles, om enhederne er aktive uden.

### Tilbagefald, hvis Hi-Fi Cable ikke virker
Kun én gratis virtuel mikrofon (VB-CABLE). Så skal "til Claude" og "til Discord" dele én enhed, hvilket bryder ekko-reglerne. Det accepteres ikke. Alternativet ville være at bede brugeren donere til A+B, og det står i så fald ærligt i `KNOWN_ISSUES.md`.

## D2 – CI-runnere
- `windows-latest` og `windows-2025` som krævet, plus `windows-2022` (build 20348, samme kernegeneration som Windows 10 21H2) som det nærmeste, GitHub tilbyder for Windows 10. Ægte Windows 10-klient findes ikke som hosted runner. Se `KNOWN_ISSUES.md`.
- Runnere er VM'er. Secure Boot rapporteres i hver kørsel, så det er tydeligt, hvad CI kan og ikke kan bevise om kernesignaturpolitik.
