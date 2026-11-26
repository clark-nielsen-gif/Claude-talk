# Kendte begrænsninger

## K1 – Windows 10 understøttes ikke
At optage ét programs lyd kræver Windows build 20348 eller nyere (`DECISIONS.md` D10). Windows 10 22H2 (19045) kan det ikke, så programmet viser beskeden "Claude Talk kræver Windows 11". **Bedste alternativ:** opdater til Windows 11. Der findes heller ingen Windows 10-runner i GitHub Actions. `windows-2022` (build 20348) dækker det ældste understøttede API-niveau.

## K2 – Runnere har test-signing slået til
Driverindlæsning på runneren beviser derfor ikke signaturen. Signaturen bevises i stedet med `signtool /kp` og Microsofts katalogsignatur på den installerede pakke (se `DECISIONS.md` D1). Secure Boot er slået fra på runnerne.

## K3 – Kun én gratis, produktionssigneret virtuel mikrofon findes – **LØST**
Løst med arkitektur 2 (`DECISIONS.md` D5): Kun Discord får en virtuel mikrofon (VB-CABLE). Claude får lyden direkte i Claude Talks indbyggede Claude-vindue. Den oprindelige to-kabel-routing til Chrome eller desktop-appen kan kun tilbydes, hvis brugeren selv har installeret et ekstra kabel (f.eks. VB-CABLE A+B efter donation).

## K4 – Claudes stemmefunktion kan ikke testes med et rigtigt login i CI
CI kan ikke logge ind på claude.ai. Indsprøjtningen testes mod en lokal testside, der efterligner en stemme-webapp (getUserMedia, enumerateDevices, Web Audio, streng CSP). Hvis claude.ai ændrer, hvordan den henter mikrofonen, kan det kræve en opdatering. Det rigtige forløb står i `MANUAL_TEST.md`.

## K5 – "Fortsæt med Google" virker ikke i Claude-vinduet
Google blokerer login i indlejrede browsere. Brug login med e-mail i stedet. Vinduet viser en vejledning, når det sker.

## K6 – Mikrofonen i e2e-testen er simuleret
CI-runnere har ingen mikrofon, der kan spille en kendt tone. I e2e-testen leveres "mikrofonen" derfor som et testprograms lyd. Optagelse fra en rigtig optageenhed er testet separat (`audio-e2e`, `render_capture_cable`), og den rigtige mikrofon testes i `MANUAL_TEST.md`.
