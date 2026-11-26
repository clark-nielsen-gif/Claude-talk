# Kendte begrænsninger

## K1 – Ingen ægte Windows 10-runner
GitHub Actions tilbyder kun Windows Server-billeder. `windows-2022` bruges som det nærmeste til Windows 10. En ægte test på Windows 10 22H2 skal ske manuelt efter `MANUAL_TEST.md`.

## K2 – Runnere har test-signing slået til
Driverindlæsning på runneren beviser derfor ikke signaturen. Signaturen bevises i stedet med `signtool /kp` og Microsofts katalogsignatur på den installerede pakke (se `DECISIONS.md` D1). Secure Boot er slået fra på runnerne.

## K3 – Kun én gratis, produktionssigneret virtuel mikrofon findes (blokerer M0)
**Årsag:** VB-CABLE (gratis, Microsoft-attesteret) giver én mikrofon. Claude Talk kræver to: én som Discord bruger (dig + Claude) og én som Claude bruger (dig + Discord). Hvis de deles, hører Claude sig selv, og Discord hører sig selv. Det er netop det ekko, programmet skal forhindre. Hi-Fi Cable er afvist (gammel signatur, dialog, mister Windows' tillid fra april 2026). VirtualDrivers-driveren er ikke Microsoft-signeret.

**Muligheder, rangeret:**
1. **VB-CABLE A+B (anbefalet).** Officielle, produktionssignerede VB-Audio-kabler. Kræver en engangsdonation af selvvalgt størrelse i VB-Audios webshop, som giver et personligt downloadlink. Claude Talk installerer den gratis VB-CABLE automatisk. For den anden mikrofon viser programmet én knap: "Hent det andet lydkabel". Den åbner webshoppen, og når zip-filen ligger i Overførsler, finder og installerer Claude Talk den selv. Det er ikke helt "kun Setup.exe", men det er tæt på og helt uden tekniske trin.
2. **Voicemeeter Banana som andet kabel (gratis).** Giver to virtuelle mikrofoner, men Voicemeeter-programmet skal køre skjult i baggrunden hele tiden. Det giver mere latenstid, VB-Audios donationspåmindelser og en tung afhængighed.
3. **Egen driver med Microsoft-attestation.** Den mest "egen" løsning, men den kræver et EV-kodesigneringscertifikat (ca. 2.000–3.000 kr. om året) og en Microsoft Partner Center-konto med identitetskontrol. Det kan kun ejeren af projektet skaffe.
