; The application uses a stable registry name without spaces.
; Preserve it during upgrades, remove it on explicit uninstall.
; The legacy name is also removed: installs predating the CC Usage rename
; wrote "AIUsageIsland", and leaving it behind would keep launching a
; product that no longer exists on this machine.
!macro NSIS_HOOK_POSTUNINSTALL
  ${If} $UpdateMode <> 1
    DeleteRegValue HKCU "Software\Microsoft\Windows\CurrentVersion\Run" "CCUsage"
    DeleteRegValue HKCU "Software\Microsoft\Windows\CurrentVersion\Run" "AIUsageIsland"
  ${EndIf}
!macroend
