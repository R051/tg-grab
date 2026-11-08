; A new icon path per version avoids reusing Windows' cached executable icon.
; Update existing shortcuts in place, preserving their target, AppUserModelID
; and the user's choice of which shortcuts to keep or pin.
Var BrandIconsInstalled
Var BrandIconPath
Var BrandMainPath
Var BrandName
Var BrandMenuFolder

!macro RefreshBrandShortcut shortcut
  StrCpy $R9 "${shortcut}"
  !insertmacro IsShortcutTarget "$R9" "$BrandMainPath"
  Pop $0
  ${If} $0 = 1
    !insertmacro ComHlpr_CreateInProcInstance ${CLSID_ShellLink} ${IID_IShellLink} r0 ""
    ${If} $0 P<> 0
      ${IUnknown::QueryInterface} $0 '("${IID_IPersistFile}", .r1)'
      ${If} $1 P<> 0
        ${IPersistFile::Load} $1 '(w "$R9", ${STGM_READWRITE})i.r2'
        ${If} $2 >= 0
          ${IShellLink::SetIconLocation} $0 '(w "$BrandIconPath", 0)i.r2'
          ${If} $2 >= 0
            ${IPersistFile::Save} $1 '(w "$R9", 1)i.r2'
            ${If} $2 >= 0
              ; Notify only the changed item; do not clear the system icon cache.
              System::Call 'shell32::SHChangeNotify(i 0x2000, i 0x0005, w "$R9", p 0)'
            ${EndIf}
          ${EndIf}
        ${EndIf}
        ${IUnknown::Release} $1 ""
      ${EndIf}
      ${IUnknown::Release} $0 ""
    ${EndIf}
  ${EndIf}
!macroend

Function RefreshBrandShortcuts
  ${If} $BrandIconsInstalled != 1
    Return
  ${EndIf}
  !insertmacro RefreshBrandShortcut "$DESKTOP\$BrandName.lnk"
  !insertmacro RefreshBrandShortcut "$SMPROGRAMS\$BrandName.lnk"
  !insertmacro RefreshBrandShortcut "$SMPROGRAMS\$BrandMenuFolder\$BrandName.lnk"

  ; A pinned shortcut can have a different filename. Check its target first.
  FindFirst $R6 $R7 "$APPDATA\Microsoft\Internet Explorer\Quick Launch\User Pinned\TaskBar\*.lnk"
  ${If} ${Errors}
    Return
  ${EndIf}
  ${DoWhile} $R7 != ""
    !insertmacro RefreshBrandShortcut "$APPDATA\Microsoft\Internet Explorer\Quick Launch\User Pinned\TaskBar\$R7"
    FindNext $R6 $R7
  ${Loop}
  FindClose $R6
FunctionEnd

!macro NSIS_HOOK_POSTINSTALL
  ; Tauri includes hooks before its own defines and variables, so initialize
  ; these here rather than referring to them in the functions above.
  StrCpy $BrandIconPath "$INSTDIR\branding\tg-grab-${VERSION}.ico"
  StrCpy $BrandMainPath "$INSTDIR\${MAINBINARYNAME}.exe"
  StrCpy $BrandName "${PRODUCTNAME}"
  StrCpy $BrandMenuFolder "$AppStartMenuFolder"
  StrCpy $BrandIconsInstalled 1
  WriteRegStr SHCTX "${UNINSTKEY}" "DisplayIcon" '$\"$INSTDIR\branding\tg-grab-${VERSION}.ico$\"'
  Call RefreshBrandShortcuts
!macroend

; The interactive finish page can create the desktop shortcut after POSTINSTALL.
Function .onGUIEnd
  Call RefreshBrandShortcuts
FunctionEnd
