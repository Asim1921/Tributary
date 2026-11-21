; Removes what the app registered with browsers, so uninstalling leaves nothing behind.
!macro NSIS_HOOK_POSTUNINSTALL
  DeleteRegKey HKCU "Software\Google\Chrome\NativeMessagingHosts\app.tributary.capture"
  DeleteRegKey HKCU "Software\Microsoft\Edge\NativeMessagingHosts\app.tributary.capture"
  DeleteRegKey HKCU "Software\BraveSoftware\Brave-Browser\NativeMessagingHosts\app.tributary.capture"
  DeleteRegKey HKCU "Software\Vivaldi\NativeMessagingHosts\app.tributary.capture"
  DeleteRegKey HKCU "Software\Chromium\NativeMessagingHosts\app.tributary.capture"
  DeleteRegKey HKCU "Software\Mozilla\NativeMessagingHosts\app.tributary.capture"
  DeleteRegKey HKCU "Software\Classes\Tributary.Magnet"
  DeleteRegKey HKCU "Software\Tributary"
  DeleteRegValue HKCU "Software\RegisteredApplications" "Tributary"
!macroend
