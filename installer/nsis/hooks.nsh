; TunedUp NSIS installer hooks (FR-15, FR-14, NFR-08, NFR-09, ADR 0012).
;
; Hooked into Tauri's NSIS template via bundle.windows.nsis.installerHooks.
; Variables used from the template: $INSTDIR, $UpdateMode (1 when the
; updater runs the installer with /UPDATE), $PassiveMode (/P).
;
; Install:   silently install VB-Cable if missing (one UAC prompt - the
;            installer itself is per-machine/elevated), record that we did,
;            keep Windows from switching the default speakers or mic to the
;            cable, and ask for a reboot only when the cable doesn't come up
;            without one (the app then reopens in the setup wizard).
; Uninstall: restore the user's default microphone, remove the user's
;            startup entries, then offer to remove VB-Cable only if this
;            installer put it there - naming the apps still recording from
;            it, and moving every default device off it first.
;
; The installer runs elevated, so its HKCU is the elevated account's. Under
; Windows 11 Administrator Protection, or when another admin approves the UAC
; prompt, that is not the signed-in user: per-user registry values go through
; user-registry.ps1, which writes the signed-in user's hive.

!define VT_REGKEY "Software\TunedUp"
!define VT_RUNONCE "Software\Microsoft\Windows\CurrentVersion\RunOnce"
; NSIS is 32-bit: use the 64-bit PowerShell (sysnative) so the driver setup,
; PnP queries and HKLM writes all see the native 64-bit system.
!define VT_PS '"$WINDIR\sysnative\WindowsPowerShell\v1.0\powershell.exe" -NoProfile -NonInteractive -ExecutionPolicy Bypass -File'

; "yes" once the user agreed to remove VB-Cable (asked before the app's
; files are gone, acted on after).
Var VtRemoveCable

!macro NSIS_HOOK_PREINSTALL
!macroend

!macro NSIS_HOOK_POSTINSTALL
  ${If} $UpdateMode <> 1
    ReadEnvStr $R9 "TUNEDUP_SKIP_VBCABLE"
    ${If} $R9 == "1"
      DetailPrint "Skipping VB-Cable (TUNEDUP_SKIP_VBCABLE=1)."
    ${Else}
      DetailPrint "Checking for the VB-Cable virtual audio driver..."
      nsExec::ExecToLog '${VT_PS} "$INSTDIR\installer\install-vbcable.ps1" -Action Install -PackDir "$INSTDIR\installer\vbcable" -RegistryKey "HKLM:\${VT_REGKEY}" -App "$INSTDIR\${MAINBINARYNAME}.exe"'
      Pop $0
      ${If} $0 == 3010
        DetailPrint "VB-Cable installed. Windows needs a restart to finish."
        SetRebootFlag true
        ; After the reboot, open straight into the setup wizard.
        nsExec::ExecToLog '${VT_PS} "$INSTDIR\installer\user-registry.ps1" -Action SetRunOnce -Exe "$INSTDIR\${MAINBINARYNAME}.exe"'
        Pop $1
        ${If} $1 != 0
          WriteRegStr HKCU "${VT_RUNONCE}" "TunedUpSetup" '"$INSTDIR\${MAINBINARYNAME}.exe" --first-run'
        ${EndIf}
      ${ElseIf} $0 == 0
        DetailPrint "VB-Cable is ready."
      ${Else}
        DetailPrint "VB-Cable could not be installed automatically (code $0). The setup wizard can try again."
      ${EndIf}
    ${EndIf}
  ${EndIf}
!macroend

!macro NSIS_HOOK_PREUNINSTALL
  ; NFR-08: put the user's default recording devices back before anything
  ; is removed. The app restores on quit too; this covers crashes.
  ${If} ${FileExists} "$INSTDIR\${MAINBINARYNAME}.exe"
    nsExec::Exec '"$INSTDIR\${MAINBINARYNAME}.exe" --restore-defaults'
    Pop $0
  ${EndIf}
  StrCpy $VtRemoveCable "no"
  ${If} $UpdateMode <> 1
    InitPluginsDir
    DeleteRegValue HKCU "${VT_RUNONCE}" "TunedUpSetup"
    ; The template only clears the elevated account's autostart entry.
    ${If} ${FileExists} "$INSTDIR\installer\user-registry.ps1"
      nsExec::ExecToLog '${VT_PS} "$INSTDIR\installer\user-registry.ps1" -Action Cleanup'
      Pop $0
    ${EndIf}
    ; Ask about VB-Cable now, while tunedup.exe can still say who uses it
    ; and move the default devices off it (ADR 0012).
    SetRegView 64
    ReadRegDWORD $R0 HKLM "${VT_REGKEY}" "InstalledVBCable"
    SetRegView default
    ${If} $R0 == 1
    ${AndIf} $PassiveMode <> 1
    ${AndIfNot} ${Silent}
      StrCpy $R2 ""
      ${If} ${FileExists} "$INSTDIR\${MAINBINARYNAME}.exe"
        nsExec::Exec '"$INSTDIR\${MAINBINARYNAME}.exe" --cable users "$PLUGINSDIR\vt-users.txt"'
        Pop $0
        ClearErrors
        FileOpen $R3 "$PLUGINSDIR\vt-users.txt" r
        ${IfNot} ${Errors}
          FileRead $R3 $R2
          FileClose $R3
        ${EndIf}
      ${EndIf}
      StrCpy $R4 "TunedUp installed the VB-Cable virtual audio driver.$\r$\n$\r$\nRemove VB-Cable as well? Choose No if other apps use it."
      ${If} $R2 != ""
        StrCpy $R4 "$R4$\r$\n$\r$\nRecording from CABLE Output right now: $R2. If you remove it, switch their microphone back to your real one."
      ${EndIf}
      MessageBox MB_YESNO|MB_ICONQUESTION "$R4" /SD IDNO IDNO vt_keep_cable
      StrCpy $VtRemoveCable "yes"
      vt_keep_cable:
    ${EndIf}
    ${If} $VtRemoveCable == "yes"
    ${AndIf} ${FileExists} "$INSTDIR\${MAINBINARYNAME}.exe"
      ; Windows would otherwise pick replacement defaults on its own.
      nsExec::Exec '"$INSTDIR\${MAINBINARYNAME}.exe" --cable release'
      Pop $0
    ${EndIf}
    ; Resources are deleted before POSTUNINSTALL runs, so stage the VB-Cable
    ; helper (and any saved driver pack) in the auto-cleaned plugins dir.
    CreateDirectory "$PLUGINSDIR\vt"
    CopyFiles /SILENT "$INSTDIR\installer\*.*" "$PLUGINSDIR\vt"
  ${EndIf}
!macroend

!macro NSIS_HOOK_POSTUNINSTALL
  ${If} $UpdateMode <> 1
    ${If} $VtRemoveCable == "yes"
      nsExec::ExecToLog '${VT_PS} "$PLUGINSDIR\vt\install-vbcable.ps1" -Action Uninstall -PackDir "$PLUGINSDIR\vt\vbcable" -RegistryKey "HKLM:\${VT_REGKEY}"'
      Pop $0
      ${If} $0 == 3010
        SetRebootFlag true
      ${EndIf}
    ${EndIf}
    ; Downloaded driver packs are not in Tauri's resource list; clean up.
    RMDir /r "$INSTDIR\installer"
    RMDir "$INSTDIR"
    ; Keep the InstalledVBCable marker only while the driver is still ours.
    SetRegView 64
    ReadRegDWORD $R0 HKLM "${VT_REGKEY}" "InstalledVBCable"
    ${If} $R0 != 1
    ${OrIf} $VtRemoveCable == "yes"
      DeleteRegKey HKLM "${VT_REGKEY}"
    ${EndIf}
    SetRegView default
  ${EndIf}
!macroend
