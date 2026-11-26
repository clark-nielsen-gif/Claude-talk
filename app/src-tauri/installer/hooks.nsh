; Claude Talk NSIS hooks (Tauri 2). The installer is per-machine, so these
; run elevated inside the single UAC prompt of the installer.

!macro NSIS_HOOK_POSTINSTALL
  DetailPrint "Claude Talk: Discords mikrofon sættes op / setting up Discord's microphone"
  install_cable:
  nsExec::ExecToLog '"$INSTDIR\claude-talk.exe" --install-cable'
  Pop $0
  ${If} $0 == 2
    MessageBox MB_RETRYCANCEL|MB_ICONEXCLAMATION "Claude Talk skal bruge internettet et øjeblik for at hente Discords mikrofon fra VB-Audio. Tjek din forbindelse, og tryk Prøv igen.$\r$\n$\r$\nClaude Talk needs the internet for a moment to download Discord's microphone from VB-Audio. Check your connection and press Retry." /SD IDCANCEL IDRETRY install_cable
  ${ElseIf} $0 != 0
    MessageBox MB_OK|MB_ICONEXCLAMATION "Discords mikrofon kunne ikke sættes op. Åbn Claude Talk – den forklarer, hvad du kan gøre.$\r$\n$\r$\nDiscord's microphone could not be set up. Open Claude Talk – it explains what to do." /SD IDOK
  ${EndIf}
!macroend

!macro NSIS_HOOK_PREUNINSTALL
  DetailPrint "Claude Talk: lydindstillinger gendannes / restoring sound settings"
  nsExec::ExecToLog '"$INSTDIR\claude-talk.exe" --uninstall-cleanup'
  Pop $0
  ; The Claude window's login (claude.ai cookies) and the settings stay
  ; unless the user says so; a silent uninstall (/S) keeps them.
  MessageBox MB_YESNO|MB_ICONQUESTION "Vil du også slette dine indstillinger og logge ud af Claude i Claude Talk?$\r$\n$\r$\nDo you also want to delete your settings and sign out of Claude in Claude Talk?" /SD IDNO IDNO keep_user_data
    SetShellVarContext current
    RMDir /r "$LOCALAPPDATA\Claude Talk"
    RMDir /r "$APPDATA\Claude Talk"
    SetShellVarContext all
  keep_user_data:
!macroend
