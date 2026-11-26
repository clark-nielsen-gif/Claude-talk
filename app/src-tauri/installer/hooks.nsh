; Claude Talk NSIS hooks (Tauri 2). The installer is per-machine, so these
; run elevated inside the single UAC prompt of the installer.
; Messages follow the installer's language: Danish (1030) or English.

!macro NSIS_HOOK_POSTINSTALL
  ${If} $LANGUAGE == 1030
    DetailPrint "Claude Talk-mikrofonen hentes og sættes op (det kan tage et minut)"
  ${Else}
    DetailPrint "Downloading and setting up the Claude Talk microphone (this can take a minute)"
  ${EndIf}
  install_cable:
  nsExec::ExecToLog '"$INSTDIR\claude-talk.exe" --install-cable'
  Pop $0
  ${If} $0 == 2
    ${If} $LANGUAGE == 1030
      MessageBox MB_RETRYCANCEL|MB_ICONEXCLAMATION "Claude Talk skal bruge internettet et øjeblik for at hente Claude Talk-mikrofonen fra VB-Audio. Tjek din forbindelse, og tryk Prøv igen." /SD IDCANCEL IDRETRY install_cable
    ${Else}
      MessageBox MB_RETRYCANCEL|MB_ICONEXCLAMATION "Claude Talk needs the internet for a moment to download the Claude Talk microphone from VB-Audio. Check your connection and press Retry." /SD IDCANCEL IDRETRY install_cable
    ${EndIf}
  ${ElseIf} $0 == 8
    ${If} $LANGUAGE == 1030
      MessageBox MB_OK|MB_ICONEXCLAMATION "Claude Talk kræver Windows 11. Denne pc har en ældre Windows, så Claude Talk-mikrofonen er ikke installeret. Du kan afinstallere Claude Talk igen under Indstillinger → Apps." /SD IDOK
    ${Else}
      MessageBox MB_OK|MB_ICONEXCLAMATION "Claude Talk needs Windows 11. This PC has an older Windows, so the Claude Talk microphone was not installed. You can uninstall Claude Talk again in Settings → Apps." /SD IDOK
    ${EndIf}
  ${ElseIf} $0 == 7
    ${If} $LANGUAGE == 1030
      MessageBox MB_OK|MB_ICONINFORMATION "Claude Talk-mikrofonen er installeret, men slået fra i Windows. Åbn Claude Talk. Programmet viser dig, hvordan du slår den til." /SD IDOK
    ${Else}
      MessageBox MB_OK|MB_ICONINFORMATION "The Claude Talk microphone is installed but turned off in Windows. Open Claude Talk. It shows you how to turn it on." /SD IDOK
    ${EndIf}
  ${ElseIf} $0 == 6
    ${If} $LANGUAGE == 1030
      MessageBox MB_OK|MB_ICONEXCLAMATION "Claude Talk-mikrofonen findes ikke til pc'er med ARM-processor (f.eks. Snapdragon) endnu. Claude Talk kan derfor ikke bruges på denne pc." /SD IDOK
    ${Else}
      MessageBox MB_OK|MB_ICONEXCLAMATION "The Claude Talk microphone isn't available for PCs with an ARM processor (e.g. Snapdragon) yet. Claude Talk can't be used on this PC." /SD IDOK
    ${EndIf}
  ${ElseIf} $0 != 0
    ${If} $LANGUAGE == 1030
      MessageBox MB_OK|MB_ICONEXCLAMATION "Claude Talk-mikrofonen kunne ikke sættes op. Åbn Claude Talk. Programmet forklarer dig, hvad du kan gøre." /SD IDOK
    ${Else}
      MessageBox MB_OK|MB_ICONEXCLAMATION "The Claude Talk microphone could not be set up. Open Claude Talk. It explains what you can do." /SD IDOK
    ${EndIf}
  ${EndIf}
!macroend

!macro NSIS_HOOK_PREUNINSTALL
  ; An installer that replaces this version (an upgrade or a reinstall) runs
  ; this uninstaller in place (_?=$INSTDIR); an uninstall from Windows'
  ; settings copies itself to a temporary folder first. Only a real uninstall
  ; removes the Claude Talk microphone and asks about the user's data.
  ${If} $EXEDIR != $INSTDIR
    ${If} $LANGUAGE == 1030
      DetailPrint "Lydindstillingerne gendannes"
    ${Else}
      DetailPrint "Restoring sound settings"
    ${EndIf}
    nsExec::ExecToLog '"$INSTDIR\claude-talk.exe" --uninstall-cleanup'
    Pop $0
    ; The Claude window's login (claude.ai cookies) and the settings go only
    ; if the user ticks Tauri's own "delete data" box on the uninstall page
    ; (its Danish text: "Slet også indstillinger og Claude-login"). Tauri's
    ; box removes only its own folders, so ours are removed here.
    ${If} $DeleteAppDataCheckboxState = 1
      SetShellVarContext current
      RMDir /r "$LOCALAPPDATA\Claude Talk"
      RMDir /r "$APPDATA\Claude Talk"
      SetShellVarContext all
    ${EndIf}
  ${EndIf}
!macroend
