; Clipdeck registers itself to start at login under this name (see
; src/autostart.rs). Uninstalling removes that entry so Windows doesn't keep
; trying to start an exe that's gone. An update reinstall may run this too;
; the new version registers itself again when it starts.
!macro NSIS_HOOK_POSTUNINSTALL
  DeleteRegValue HKCU "Software\Microsoft\Windows\CurrentVersion\Run" "Clipdeck"
  DeleteRegValue HKCU "Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run" "Clipdeck"
!macroend
