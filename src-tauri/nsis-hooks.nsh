; Al desinstalar LFind, las carpetas deben seguir abriéndose: se retira todo lo que LFind escribió en el Registro.
; Debe coincidir con crates/nova-core/src/winshell.rs.

!macro LFIND_RESTORE_DEFAULT KIND
  ReadRegStr $R0 HKCU "Software\Classes\${KIND}\shell" ""
  StrCmp $R0 "LFindOpen" 0 lfind_done_${KIND}
  ReadRegStr $R1 HKCU "Software\LFind\Integration" "${KIND}"
  StrCmp $R1 "" 0 lfind_write_${KIND}
  DeleteRegValue HKCU "Software\Classes\${KIND}\shell" ""
  Goto lfind_done_${KIND}
  lfind_write_${KIND}:
  WriteRegStr HKCU "Software\Classes\${KIND}\shell" "" "$R1"
  lfind_done_${KIND}:
  DeleteRegKey HKCU "Software\Classes\${KIND}\shell\LFindOpen"
!macroend

!macro NSIS_HOOK_POSTUNINSTALL
  !insertmacro LFIND_RESTORE_DEFAULT "Directory"
  !insertmacro LFIND_RESTORE_DEFAULT "Drive"
  DeleteRegKey HKCU "Software\Classes\Directory\shell\LFind"
  DeleteRegKey HKCU "Software\Classes\Directory\Background\shell\LFind"
  DeleteRegKey HKCU "Software\Classes\Drive\shell\LFind"
  DeleteRegValue HKCU "Software\Microsoft\Windows\CurrentVersion\Run" "LFind"
  DeleteRegKey HKCU "Software\LFind\Integration"
  DeleteRegKey /ifempty HKCU "Software\LFind"
!macroend
