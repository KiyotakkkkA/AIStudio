!define LEGACY_PRODUCT "ZVS AI Studio"
!define LEGACY_INSTALL_KEY "Software\ZVS\${LEGACY_PRODUCT}"
!define LEGACY_UNINSTALL_KEY "Software\Microsoft\Windows\CurrentVersion\Uninstall\${LEGACY_PRODUCT}"
!define RUN_KEY "Software\Microsoft\Windows\CurrentVersion\Run"

Var LegacyInstall

!macro NSIS_HOOK_PREINSTALL
  ReadRegStr $LegacyInstall HKCU "${LEGACY_INSTALL_KEY}" ""
  ReadRegStr $0 HKCU "${LEGACY_UNINSTALL_KEY}" "MainBinaryName"
  ${If} $LegacyInstall != ""
  ${AndIf} $0 == "${MAINBINARYNAME}.exe"
  ${AndIf} ${FileExists} "$LegacyInstall\${MAINBINARYNAME}.exe"
    StrCpy $INSTDIR "$LegacyInstall"
    SetOutPath "$INSTDIR"
  ${Else}
    StrCpy $LegacyInstall ""
  ${EndIf}
!macroend

!macro RenameDesktopShortcut FOLDER
  !insertmacro IsShortcutTarget "${FOLDER}\${LEGACY_PRODUCT}.lnk" "$INSTDIR\${MAINBINARYNAME}.exe"
  Pop $0
  ${If} $0 = 1
    ${IfNot} ${FileExists} "${FOLDER}\${PRODUCTNAME}.lnk"
      Rename "${FOLDER}\${LEGACY_PRODUCT}.lnk" "${FOLDER}\${PRODUCTNAME}.lnk"
    ${Else}
      Delete "${FOLDER}\${LEGACY_PRODUCT}.lnk"
    ${EndIf}
  ${EndIf}
!macroend

!macro NSIS_HOOK_POSTINSTALL
  ${If} $LegacyInstall != ""
    !insertmacro RenameDesktopShortcut "$SMPROGRAMS"
    !insertmacro RenameDesktopShortcut "$DESKTOP"
    ReadRegStr $0 HKCU "${RUN_KEY}" "${LEGACY_PRODUCT}"
    ${If} $0 != ""
      WriteRegStr HKCU "${RUN_KEY}" "${PRODUCTNAME}" "$0"
      DeleteRegValue HKCU "${RUN_KEY}" "${LEGACY_PRODUCT}"
    ${EndIf}
    DeleteRegKey HKCU "${LEGACY_UNINSTALL_KEY}"
    DeleteRegKey HKCU "${LEGACY_INSTALL_KEY}"
  ${EndIf}
!macroend

!macro NSIS_HOOK_POSTUNINSTALL
  ${If} $DeleteAppDataCheckboxState = 1
  ${AndIf} $UpdateMode <> 1
    SetShellVarContext current
    RMDir /r "$LOCALAPPDATA\ZVS\AI Studio Desktop"
    RMDir /r "$LOCALAPPDATA\ZVS\AIStudio"
  ${EndIf}
!macroend
