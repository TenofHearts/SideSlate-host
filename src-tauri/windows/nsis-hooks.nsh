!macro NSIS_HOOK_POSTINSTALL
  IfSilent SideSlateDesktopShortcutDone
  MessageBox MB_YESNO "Create a desktop shortcut?" IDNO SideSlateDesktopShortcutDone
  CreateShortcut "$DESKTOP\SideSlate.lnk" "$INSTDIR\SideSlate.exe"

SideSlateDesktopShortcutDone:
!macroend

!macro NSIS_HOOK_POSTUNINSTALL
  Delete "$DESKTOP\SideSlate.lnk"
!macroend
