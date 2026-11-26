# Review-runder

Hver runde laves af fem nye, uafhængige reviewere (ingen kender de tidligere runder): **kode**, **lyd**, **design**, **"min mor"** (ikke-teknisk dansk bruger) og **"bryd det"** (prøver at få programmet til at fejle). Projektet er færdigt, når to runder i træk er rene, altså uden blocker- og major-fund.

## Runde 1 – 2026-11-26 – ikke ren

| Rolle | Resultat | Vigtigste fund |
|---|---|---|
| Kode | 3 major, 8 minor | Når Claude Talk lukkes fra bakken, peger Discord stadig på kablet (Discord bliver stum). Installerens tillidskæde er for svag (mappe i ProgramData kan plantes af almindelige brugere, hash-afvigelse accepteres, enhver signatur godtages). Opdaterings-signaturen passer ikke, når SignPath har gen-signeret installeren. |
| Lyd | 7 major, 8 minor | Stilhed fyldes først efter 30 ms, så jitterbufferne vokser, når en kilde tier. Render-bufferen holdes fuld (ca. 22 ms ekstra). Ekkodæmperens tråd har ingen MMCSS og allokerer. Referencen mangler kilder i "Claude i Chrome"-tilstand. "Stereo Mix" kan omgå låsene. Kommandoer kan gå tabt, når en udgang mangler længe. Claude-broen allokerer i sin lydtråd. |
| Design | 7 major, 8 minor | Kortene modsiger fejlbeskeder og den store kontakt. Kontrast: hvid tekst på knapperne (3,1:1), lille grå tekst, flueben i mørkt tema og kontakten i "fra". Deaktiveret knap ligner en aktiv. Dialogen forsvinder i mørkt tema. |
| "Min mor" | 8 major, 7 minor | Kort, der modsiger fejl. "Ingen ekko" (forkert dansk). "Du ville høre sig selv". "Send den til os" uden adresse. Tekniske tal i Hjælp. Ingen knap til Claude-vinduet. README-link til en GitHub-side og en skræmmende SmartScreen-forklaring. |
| "Bryd det" | 7 major, 8 minor | En anden VB-Audio-enhed (Hi-Fi Cable, brugerens eget VB-CABLE) kan blive omdøbt og overtaget. Et nyt standard-headset følges ikke. Mikrofonen er åben hele tiden (Bluetooth i håndfri-tilstand). Stereo Mix omgår låsene. Installer-tillid. Chrome-tilstand plus højttalere giver ekko. VoiceMeeter fejlklassificeres. |

Rettelser fra runde 1 står i commit-historikken med henvisning "review 1". Fund, der ikke rettes, står med begrundelse i `KNOWN_ISSUES.md`.
