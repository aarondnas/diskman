; Diskman: hooks for the NSIS installer of Tauri (tauri.windows.conf.json: bundle > windows > nsis >
; installerHooks). Saved as UTF-8 with BOM: NSIS reads UTF-8 only with it.
;
; After installing, the installer asks once whether Diskman starts when the user signs in. Yes writes
; the same value of the Run key that the app writes and reads (src-tauri/src/platform/autostart.rs,
; sys/windows/autostart.rs); either answer is noted in HKCU\Software\Diskman, so updates and the
; app do not ask again. Silent installs (/S) answer no. Uninstalling removes both, but not when
; the uninstaller runs as part of an update (/UPDATE).
;
; Tauri includes this file before the languages are defined, so the question is chosen at run time
; from $LANGUAGE (the installer's language, from the Windows display language) instead of with
; LangString. The texts match the app's languages (src/i18n/locales).

!define DISKMAN_RUN_KEY "Software\Microsoft\Windows\CurrentVersion\Run"
!define DISKMAN_APPROVED_KEY "Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run"
!define DISKMAN_APP_KEY "Software\Diskman"

; The question in $R0
!macro DISKMAN_AUTOSTART_QUESTION
  ${If} $LANGUAGE == 1031
    StrCpy $R0 "Diskman beim Anmelden bei Windows starten?$\n$\nDiskman läuft dann ohne Fenster im Infobereich: Es führt deine Zeitpläne aus und holt Backups nach, die verpasst wurden, während der Computer aus war. Du kannst das später in den Einstellungen von Diskman ändern."
  ${ElseIf} $LANGUAGE == 1036
    StrCpy $R0 "Démarrer Diskman à l'ouverture de session Windows ?$\n$\nDiskman tourne alors sans fenêtre dans la zone de notification : il exécute vos planifications et rattrape les sauvegardes manquées pendant que l'ordinateur était éteint. Vous pourrez changer cela plus tard dans les paramètres de Diskman."
  ${ElseIf} $LANGUAGE == 1034
  ${OrIf} $LANGUAGE == 3082
    StrCpy $R0 "¿Iniciar Diskman al iniciar sesión en Windows?$\n$\nDiskman se ejecuta entonces sin ventana en el área de notificación: ejecuta tus programaciones y recupera las copias que se perdieron mientras el equipo estaba apagado. Puedes cambiarlo más tarde en los ajustes de Diskman."
  ${ElseIf} $LANGUAGE == 1040
    StrCpy $R0 "Avviare Diskman all'accesso a Windows?$\n$\nDiskman funziona allora senza finestra nell'area di notifica: esegue le tue pianificazioni e recupera i backup persi mentre il computer era spento. Puoi cambiarlo più tardi nelle impostazioni di Diskman."
  ${ElseIf} $LANGUAGE == 1043
    StrCpy $R0 "Diskman starten bij het aanmelden bij Windows?$\n$\nDiskman draait dan zonder venster in het systeemvak: het voert je planningen uit en haalt back-ups in die gemist zijn terwijl de computer uit stond. Je kunt dit later wijzigen in de instellingen van Diskman."
  ${ElseIf} $LANGUAGE == 2070
  ${OrIf} $LANGUAGE == 1046
    StrCpy $R0 "Iniciar o Diskman ao iniciar sessão no Windows?$\n$\nO Diskman é então executado sem janela na área de notificação: executa os seus agendamentos e recupera os backups perdidos enquanto o computador estava desligado. Pode alterar isto mais tarde nas definições do Diskman."
  ${Else}
    StrCpy $R0 "Start Diskman when you sign in to Windows?$\n$\nDiskman then runs without a window in the notification area: it runs your schedules and catches up backups that were missed while the computer was off. You can change this later in the settings of Diskman."
  ${EndIf}
!macroend

!macro NSIS_HOOK_POSTINSTALL
  ReadRegStr $R1 HKCU "${DISKMAN_RUN_KEY}" "Diskman"
  ${If} $R1 != ""
    ; Already starts at login (e.g. an earlier version): keep it, pointing to this installation
    WriteRegStr HKCU "${DISKMAN_RUN_KEY}" "Diskman" '"$INSTDIR\${MAINBINARYNAME}.exe" --background'
  ${Else}
    ClearErrors
    ReadRegDWORD $R1 HKCU "${DISKMAN_APP_KEY}" "AutostartAsked"
    ${If} $R1 != 1
      !insertmacro DISKMAN_AUTOSTART_QUESTION
      MessageBox MB_YESNO|MB_ICONQUESTION "$R0" /SD IDNO IDNO diskman_autostart_answered
        WriteRegStr HKCU "${DISKMAN_RUN_KEY}" "Diskman" '"$INSTDIR\${MAINBINARYNAME}.exe" --background'
        DeleteRegValue HKCU "${DISKMAN_APPROVED_KEY}" "Diskman"
      diskman_autostart_answered:
      WriteRegDWORD HKCU "${DISKMAN_APP_KEY}" "AutostartAsked" 1
    ${EndIf}
  ${EndIf}
!macroend

!macro NSIS_HOOK_POSTUNINSTALL
  ${If} $UpdateMode <> 1
    DeleteRegValue HKCU "${DISKMAN_RUN_KEY}" "Diskman"
    DeleteRegValue HKCU "${DISKMAN_APPROVED_KEY}" "Diskman"
    DeleteRegKey HKCU "${DISKMAN_APP_KEY}"
  ${EndIf}
!macroend
