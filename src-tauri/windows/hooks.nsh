; =====================================================================
; JC Shell - NSIS installer hooks
;
; Purpose: keep the 0.2.1+ installer compatible with OLD clients (< 0.2.1)
; whose self-built updater simply runs the downloaded installer
; (interactive, no /S) and then calls app.exit(0).
;
; Those old processes may not actually terminate (system tray + the
; single-instance plugin keep them alive), which leaves the app exe file-
; locked and makes the installer's built-in "app is running" check fail.
;
; This hook runs right BEFORE the template's CheckIfAppIsRunning and
; force-closes any running instance so the installation can proceed.
;
; Note: for 0.2.1+ clients this is a no-op, because the update helper has
; already closed the app before launching the installer with /S /UPDATE /R.
; =====================================================================

!macro NSIS_HOOK_PREINSTALL
  DetailPrint "Closing running ${PRODUCTNAME} (update compatibility)..."
  !if "${INSTALLMODE}" == "currentUser"
    nsis_tauri_utils::KillProcessCurrentUser "${MAINBINARYNAME}.exe"
  !else
    nsis_tauri_utils::KillProcess "${MAINBINARYNAME}.exe"
  !endif
  Pop $R0
  Sleep 800
!macroend
