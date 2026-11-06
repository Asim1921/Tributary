; Removes what the app registered with browsers, so uninstalling leaves nothing behind.
!macro NSIS_HOOK_POSTUNINSTALL
  DeleteRegKey HKCU "Software\Google\Chrome\NativeMessagingHosts\app.tributary.capture"
  DeleteRegKey HKCU "Software\Microsoft\Edge\NativeMessagingHosts\app.tributary.capture"
  DeleteRegKey HKCU "Software\BraveSoftware\Brave-Browser\NativeMessagingHosts\app.tributary.capture"
!macroend
