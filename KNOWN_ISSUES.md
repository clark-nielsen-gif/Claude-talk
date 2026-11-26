# Kendte begrænsninger

## K1 – Ingen ægte Windows 10-runner
GitHub Actions tilbyder kun Windows Server-billeder. `windows-2022` bruges som det nærmeste til Windows 10. En ægte test på Windows 10 22H2 skal ske manuelt efter `MANUAL_TEST.md`.

## K2 – Secure Boot kan ikke bevises fuldt i CI
Hosted runnere er VM'er, hvor Secure Boot typisk er slået fra. Hvordan kernesignaturpolitikken opfører sig på en almindelig pc med Secure Boot, kan derfor kun delvis bevises i CI (signtool `/kp` og certifikatdatoer). Det skal også tjekkes manuelt.
