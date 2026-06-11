; NSIS installer for Islandora Workbench (Windows).
;
; Compile with absolute paths supplied by CI, e.g.:
;   makensis /DVERSION=1.2.3 ^
;            /DSRCEXE=C:\path\islandora-workbench.exe ^
;            /DICONFILE=C:\path\app-icon.ico ^
;            /DOUTFILE=C:\path\Islandora-Workbench-1.2.3-setup.exe ^
;            scripts\installer.nsi
;
; The installer and the installed app use assets/icons/app-icon.ico — replace
; that icon (regenerate via scripts/gen-icons.ps1) to rebrand. UNSIGNED: users
; will see a Windows SmartScreen "unknown publisher" prompt (More info > Run anyway).

Unicode true

!ifndef VERSION
  !define VERSION "0.0.0"
!endif
!ifndef SRCEXE
  !define SRCEXE "..\target\release\app.exe"
!endif
!ifndef ICONFILE
  !define ICONFILE "..\assets\icons\app-icon.ico"
!endif
!ifndef OUTFILE
  !define OUTFILE "Islandora-Workbench-${VERSION}-setup.exe"
!endif
; VIProductVersion needs a strict numeric X.X.X.X. CI derives this from VERSION
; (stripping any pre-release suffix); the fallback covers local/manual compiles.
!ifndef VIVERSION
  !define VIVERSION "0.0.0.0"
!endif

!define APPNAME   "Islandora Workbench GUI"
!define EXENAME   "islandora-workbench.exe"
!define COMPANY   "devnull03"
!define UNINSTKEY "Software\Microsoft\Windows\CurrentVersion\Uninstall\${APPNAME}"
; App-readable key: the optional component writes UvPath + ProvisionWorkbench here, and the GUI
; reads them as a fallback when the user hasn't configured paths manually.
!define REGKEY    "Software\${APPNAME}"
; uv standalone build (single self-contained exe). uv then provisions Python itself at runtime.
!define UV_URL    "https://github.com/astral-sh/uv/releases/latest/download/uv-x86_64-pc-windows-msvc.zip"

Name "${APPNAME}"
OutFile "${OUTFILE}"
InstallDir "$PROGRAMFILES64\${APPNAME}"
InstallDirRegKey HKLM "Software\${APPNAME}" "InstallDir"
RequestExecutionLevel admin
SetCompressor /SOLID lzma

!include "MUI2.nsh"

!define MUI_ICON   "${ICONFILE}"
!define MUI_UNICON "${ICONFILE}"
; No MUI_FINISHPAGE_RUN: the installer is elevated, so an auto-run would launch the app as admin and
; create the per-user workbench/venv under the wrong profile. Users launch via the Start Menu shortcut.

!insertmacro MUI_PAGE_WELCOME
!insertmacro MUI_PAGE_DIRECTORY
!insertmacro MUI_PAGE_COMPONENTS
!insertmacro MUI_PAGE_INSTFILES
!insertmacro MUI_PAGE_FINISH

!insertmacro MUI_UNPAGE_CONFIRM
!insertmacro MUI_UNPAGE_INSTFILES

!insertmacro MUI_LANGUAGE "English"

VIProductVersion "${VIVERSION}"
VIAddVersionKey "ProductName"     "${APPNAME}"
VIAddVersionKey "CompanyName"     "${COMPANY}"
VIAddVersionKey "FileDescription" "${APPNAME} Installer"
VIAddVersionKey "FileVersion"     "${VERSION}"
VIAddVersionKey "ProductVersion"  "${VERSION}"

; Leading '-' hides this from the components list; SectionIn RO forces it always-installed.
Section "-Core application" SEC_CORE
  SectionIn RO
  SetOutPath "$INSTDIR"
  File /oname=${EXENAME} "${SRCEXE}"

  ; Start Menu + Desktop shortcuts
  CreateShortcut "$SMPROGRAMS\${APPNAME}.lnk" "$INSTDIR\${EXENAME}"
  CreateShortcut "$DESKTOP\${APPNAME}.lnk"    "$INSTDIR\${EXENAME}"

  ; Uninstaller + Add/Remove Programs entry
  WriteUninstaller "$INSTDIR\Uninstall.exe"
  WriteRegStr   HKLM "Software\${APPNAME}" "InstallDir" "$INSTDIR"
  WriteRegStr   HKLM "${UNINSTKEY}" "DisplayName"     "${APPNAME}"
  WriteRegStr   HKLM "${UNINSTKEY}" "DisplayVersion"  "${VERSION}"
  WriteRegStr   HKLM "${UNINSTKEY}" "Publisher"       "${COMPANY}"
  WriteRegStr   HKLM "${UNINSTKEY}" "DisplayIcon"     "$INSTDIR\${EXENAME}"
  WriteRegStr   HKLM "${UNINSTKEY}" "UninstallString" "$INSTDIR\Uninstall.exe"
  WriteRegDWORD HKLM "${UNINSTKEY}" "NoModify" 1
  WriteRegDWORD HKLM "${UNINSTKEY}" "NoRepair" 1
SectionEnd

; Optional: provision the uv runtime (machine-wide) and flag the app to fetch the Islandora
; Workbench Python tool into the user's profile on first launch. Best-effort — never aborts the
; install; the core app above is already fully installed.
;
; NSIS quoting note: the PowerShell command is one NSIS '...' string. PowerShell's own string
; literals are written as doubled single quotes (''x''), and a literal PowerShell '$' is escaped
; as '$$' so NSIS doesn't treat it as a variable ($INSTDIR / ${UV_URL} ARE NSIS-expanded on purpose).
Section "Islandora Workbench support (Python tool)" SEC_WB
  DetailPrint "Provisioning uv runtime..."
  CreateDirectory "$INSTDIR\bin"

  nsExec::ExecToLog 'powershell.exe -NoProfile -ExecutionPolicy Bypass -Command "[Net.ServicePointManager]::SecurityProtocol=''Tls12''; $$ProgressPreference=''SilentlyContinue''; try { Invoke-WebRequest -UseBasicParsing -TimeoutSec 120 -Uri ''${UV_URL}'' -OutFile ''$INSTDIR\uv.zip''; Expand-Archive -LiteralPath ''$INSTDIR\uv.zip'' -DestinationPath ''$INSTDIR\bin'' -Force; if (-not (Test-Path ''$INSTDIR\bin\uv.exe'')) { exit 3 }; exit 0 } catch { exit 1 }"'
  Pop $0

  ; Record opt-in regardless: the app provisions workbench on first run (as the real user), and can
  ; fall back to a uv found on PATH if our bundled copy failed to download.
  WriteRegStr HKLM "${REGKEY}" "ProvisionWorkbench" "1"

  StrCmp $0 "0" wb_uv_ok wb_uv_fail
wb_uv_ok:
  DetailPrint "uv installed to $INSTDIR\bin\uv.exe"
  WriteRegStr HKLM "${REGKEY}" "UvPath" "$INSTDIR\bin\uv.exe"
  Goto wb_done
wb_uv_fail:
  DetailPrint "WARNING: uv download failed (code $0). The app will look for uv on PATH at runtime."
wb_done:
  Delete "$INSTDIR\uv.zip"
SectionEnd

!insertmacro MUI_FUNCTION_DESCRIPTION_BEGIN
  !insertmacro MUI_DESCRIPTION_TEXT ${SEC_WB} "Download the uv runtime and have the app set up Islandora Workbench automatically (workbench is fetched into your user profile on first launch). Requires an internet connection."
!insertmacro MUI_FUNCTION_DESCRIPTION_END

Section "Uninstall"
  Delete "$INSTDIR\${EXENAME}"
  Delete "$INSTDIR\Uninstall.exe"
  Delete "$INSTDIR\uv.zip"
  RMDir /r "$INSTDIR\bin"      ; bundled uv.exe
  RMDir  "$INSTDIR"
  Delete "$SMPROGRAMS\${APPNAME}.lnk"
  Delete "$DESKTOP\${APPNAME}.lnk"
  DeleteRegKey HKLM "${UNINSTKEY}"
  DeleteRegKey HKLM "Software\${APPNAME}"   ; removes InstallDir + UvPath + ProvisionWorkbench
  ; Per-user data (workbench, venv, settings) under %LOCALAPPDATA%\islandora_workbench_gui is left
  ; in place — the elevated uninstaller can't resolve the original user's profile.
  DetailPrint "Note: per-user data under %LOCALAPPDATA%\islandora_workbench_gui was left in place."
SectionEnd
