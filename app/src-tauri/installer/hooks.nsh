; Claude Talk NSIS hooks (Tauri 2). The installer is per-machine, so these
; run elevated inside the single UAC prompt of the installer.
; Messages follow the installer's language: Danish (1030) or English.

!macro NSIS_HOOK_PREINSTALL
  ; A restore helper left by an earlier Claude Talk (it gives Discord its
  ; microphone back when Discord starts) would keep the program file in use.
  nsExec::Exec 'taskkill /F /IM claude-talk.exe'
  Pop $0
!macroend

!macro NSIS_HOOK_POSTINSTALL
  ${If} $LANGUAGE == 1030
    DetailPrint "Claude Talk-mikrofonen sættes op"
  ${Else}
    DetailPrint "Setting up the Claude Talk microphone"
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
    ; The Claude window's login (claude.ai cookies) and the settings stay
    ; unless the user says so; "No" is the default button, and a silent
    ; uninstall (/S) keeps them.
    ${If} $LANGUAGE == 1030
      MessageBox MB_YESNO|MB_ICONQUESTION|MB_DEFBUTTON2 "Vil du også slette dine indstillinger og logge ud af Claude i Claude Talk?$\r$\n$\r$\nDin Claude-konto og dine samtaler bliver ikke slettet." /SD IDNO IDNO keep_user_data
    ${Else}
      MessageBox MB_YESNO|MB_ICONQUESTION|MB_DEFBUTTON2 "Do you also want to delete your settings and sign out of Claude in Claude Talk?$\r$\n$\r$\nYour Claude account and conversations are not deleted." /SD IDNO IDNO keep_user_data
    ${EndIf}
      SetShellVarContext current
      RMDir /r "$LOCALAPPDATA\Claude Talk"
      RMDir /r "$APPDATA\Claude Talk"
      SetShellVarContext all
    keep_user_data:
  ${EndIf}
!macroend
