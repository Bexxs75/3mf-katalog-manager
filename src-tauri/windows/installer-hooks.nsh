; Do not let a later MSI reuse the removed NSIS installation directory.
; PRODUCTNAME keeps preview and normal installations separate.
!macro NSIS_HOOK_POSTUNINSTALL
  DeleteRegKey HKCU "Software\${MANUFACTURER}\${PRODUCTNAME}"
!macroend
