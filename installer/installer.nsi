; Rosace Launcher installer (per-user, no admin). Built by tools/release.py with makensis:
;   makensis /DVERSION=0.5.0 /DBIN=<exe> /DICON=<ico> /DOUT=<installer.exe> installer.nsi
; Same install folder, shortcuts and uninstall entry as the former Tauri installer, so an update
; (or the Tauri -> Iced migration) replaces the old program in place.
Unicode true
!include "MUI2.nsh"
!include "FileFunc.nsh"

!define APP "Rosace Launcher"
!define EXE "rosace-launcher.exe"
!define UNINST_KEY "Software\Microsoft\Windows\CurrentVersion\Uninstall\${APP}"

Name "${APP}"
OutFile "${OUT}"
InstallDir "$LOCALAPPDATA\${APP}"
InstallDirRegKey HKCU "Software\rosace\${APP}" ""
RequestExecutionLevel user
SetCompressor /SOLID lzma
BrandingText "${APP} ${VERSION}"

VIProductVersion "${VERSION}.0"
VIAddVersionKey "ProductName" "${APP}"
VIAddVersionKey "FileDescription" "${APP} installer"
VIAddVersionKey "FileVersion" "${VERSION}"
VIAddVersionKey "ProductVersion" "${VERSION}"

!define MUI_ICON "${ICON}"
!define MUI_UNICON "${ICON}"
!insertmacro MUI_PAGE_INSTFILES
!insertmacro MUI_UNPAGE_CONFIRM
!insertmacro MUI_UNPAGE_INSTFILES
!insertmacro MUI_LANGUAGE "English"

Section "Install"
  ; The launcher may still be shutting down (self-update) or running (manual reinstall).
  nsExec::Exec 'taskkill /F /IM "${EXE}"'
  Pop $0
  Sleep 800
  SetOutPath "$INSTDIR"
  File "/oname=${EXE}" "${BIN}"
  WriteUninstaller "$INSTDIR\uninstall.exe"
  WriteRegStr HKCU "Software\rosace\${APP}" "" "$INSTDIR"
  CreateShortcut "$SMPROGRAMS\${APP}.lnk" "$INSTDIR\${EXE}"
  CreateShortcut "$DESKTOP\${APP}.lnk" "$INSTDIR\${EXE}"
  WriteRegStr HKCU "${UNINST_KEY}" "DisplayName" "${APP}"
  WriteRegStr HKCU "${UNINST_KEY}" "DisplayIcon" '"$INSTDIR\${EXE}"'
  WriteRegStr HKCU "${UNINST_KEY}" "DisplayVersion" "${VERSION}"
  WriteRegStr HKCU "${UNINST_KEY}" "Publisher" "rosace"
  WriteRegStr HKCU "${UNINST_KEY}" "InstallLocation" '"$INSTDIR"'
  WriteRegStr HKCU "${UNINST_KEY}" "UninstallString" '"$INSTDIR\uninstall.exe"'
  WriteRegDWORD HKCU "${UNINST_KEY}" "NoModify" 1
  WriteRegDWORD HKCU "${UNINST_KEY}" "NoRepair" 1
  ; /R (passed by the launcher's self-updater) = start it again.
  ${GetParameters} $0
  ClearErrors
  ${GetOptions} $0 "/R" $1
  IfErrors done
  Exec '"$INSTDIR\${EXE}"'
  done:
SectionEnd

Section "Uninstall"
  nsExec::Exec 'taskkill /F /IM "${EXE}"'
  Pop $0
  Sleep 500
  Delete "$INSTDIR\${EXE}"
  Delete "$INSTDIR\uninstall.exe"
  Delete "$SMPROGRAMS\${APP}.lnk"
  Delete "$DESKTOP\${APP}.lnk"
  RMDir "$INSTDIR"
  DeleteRegKey HKCU "${UNINST_KEY}"
  DeleteRegKey HKCU "Software\rosace\${APP}"
  ; Installed games live in %LOCALAPPDATA%\com.rosace.launcher and are kept.
SectionEnd
