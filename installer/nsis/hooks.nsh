; TunedUp NSIS installer hooks (FR-15, FR-14, NFR-08, NFR-09).
;
; Hooked into Tauri's NSIS template via bundle.windows.nsis.installerHooks.
; Variables used from the template: $INSTDIR, $UpdateMode (1 when the
; updater runs the installer with /UPDATE), $PassiveMode (/P).
;
; Install:   silently install VB-Cable if missing (one UAC prompt - the
;            installer itself is per-machine/elevated), record that we did,
;            and ask for one reboot that reopens the app in the setup wizard.
; Uninstall: restore the user's default microphone, remove the user's
;            startup entries, then offer to remove VB-Cable only if this
;            installer put it there.
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

!macro NSIS_HOOK_PREINSTALL
!macroend

!macro NSIS_HOOK_POSTINSTALL
  ${If} $UpdateMode <> 1
    ReadEnvStr $R9 "TUNEDUP_SKIP_VBCABLE"
    ${If} $R9 == "1"
      DetailPrint "Skipping VB-Cable (TUNEDUP_SKIP_VBCABLE=1)."
    ${Else}
      DetailPrint "Checking for the VB-Cable virtual audio driver..."
      nsExec::ExecToLog '${VT_PS} "$INSTDIR\installer\install-vbcable.ps1" -Action Install -PackDir "$INSTDIR\installer\vbcable" -RegistryKey "HKLM:\${VT_REGKEY}"'
      Pop $0
      ${If} $0 == 3010
        DetailPrint "VB-Cable installed. A restart is required."
        SetRebootFlag true
        ; After the reboot, open straight into the setup wizard.
        nsExec::ExecToLog '${VT_PS} "$INSTDIR\installer\user-registry.ps1" -Action SetRunOnce -Exe "$INSTDIR\${MAINBINARYNAME}.exe"'
        Pop $1
        ${If} $1 != 0
          WriteRegStr HKCU "${VT_RUNONCE}" "TunedUpSetup" '"$INSTDIR\${MAINBINARYNAME}.exe" --first-run'
        ${EndIf}
      ${ElseIf} $0 == 0
        DetailPrint "VB-Cable is already installed."
      ${Else}
        DetailPrint "VB-Cable could not be installed automatically (code $0). The setup wizard explains how to install it."
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
  ${If} $UpdateMode <> 1
    DeleteRegValue HKCU "${VT_RUNONCE}" "TunedUpSetup"
    ; The template only clears the elevated account's autostart entry.
    ${If} ${FileExists} "$INSTDIR\installer\user-registry.ps1"
      nsExec::ExecToLog '${VT_PS} "$INSTDIR\installer\user-registry.ps1" -Action Cleanup'
      Pop $0
    ${EndIf}
    ; Resources are deleted before POSTUNINSTALL runs, so stage the VB-Cable
    ; helper (and any saved driver pack) in the auto-cleaned plugins dir.
    InitPluginsDir
    CreateDirectory "$PLUGINSDIR\vt"
    CopyFiles /SILENT "$INSTDIR\installer\*.*" "$PLUGINSDIR\vt"
  ${EndIf}
!macroend

!macro NSIS_HOOK_POSTUNINSTALL
  ${If} $UpdateMode <> 1
    SetRegView 64
    ReadRegDWORD $R0 HKLM "${VT_REGKEY}" "InstalledVBCable"
    SetRegView default
    ${If} $R0 == 1
      StrCpy $R1 "no"
      ${If} $PassiveMode <> 1
      ${AndIfNot} ${Silent}
        MessageBox MB_YESNO|MB_ICONQUESTION "TunedUp installed the VB-Cable virtual audio driver.$\r$\n$\r$\nRemove VB-Cable as well? Choose No if other apps use it." /SD IDNO IDNO vt_keep_cable
        StrCpy $R1 "yes"
        vt_keep_cable:
      ${EndIf}
      ${If} $R1 == "yes"
        nsExec::ExecToLog '${VT_PS} "$PLUGINSDIR\vt\install-vbcable.ps1" -Action Uninstall -PackDir "$PLUGINSDIR\vt\vbcable" -RegistryKey "HKLM:\${VT_REGKEY}"'
        Pop $0
        ${If} $0 == 3010
          SetRebootFlag true
        ${EndIf}
      ${EndIf}
    ${EndIf}
    ; Downloaded driver packs are not in Tauri's resource list; clean up.
    RMDir /r "$INSTDIR\installer"
    RMDir "$INSTDIR"
    ; Keep the InstalledVBCable marker only if the driver is still ours.
    ${If} $R1 != "no"
    ${OrIf} $R0 != 1
      SetRegView 64
      DeleteRegKey HKLM "${VT_REGKEY}"
      SetRegView default
    ${EndIf}
  ${EndIf}
!macroend
