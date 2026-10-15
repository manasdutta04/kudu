; Kudu for Windows installer (Inno Setup 6). Built in CI:
;   iscc /DAppVersion=0.1.2 /DSourceDir=path\to\pkg kudu.iss
; Installs for the current user (no admin rights), adds a Start menu shortcut, and an entry in
; Settings → Apps to uninstall.

#ifndef AppVersion
  #define AppVersion "0.0.0"
#endif
#ifndef SourceDir
  #define SourceDir "pkg"
#endif

[Setup]
AppId={{7E3C9B52-4C1A-4D8E-9C2F-6B1D0A5E8F31}
AppName=Kudu
AppVersion={#AppVersion}
AppVerName=Kudu {#AppVersion}
AppPublisher=Kudu contributors
AppPublisherURL=https://github.com/manasdutta04/kudu
AppSupportURL=https://github.com/manasdutta04/kudu/issues
DefaultDirName={autopf}\Kudu
DefaultGroupName=Kudu
DisableProgramGroupPage=yes
DisableDirPage=yes
PrivilegesRequired=lowest
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
OutputBaseFilename=Kudu-Windows-x64-Setup
OutputDir=.
SetupIconFile=kudu.ico
UninstallDisplayIcon={app}\kudu.exe
Compression=lzma2/max
SolidCompression=yes
WizardStyle=modern
CloseApplications=force
LicenseFile={#SourceDir}\LICENSE.txt

[Files]
Source: "{#SourceDir}\kudu.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#SourceDir}\LICENSE.txt"; DestDir: "{app}"; Flags: ignoreversion

[Icons]
Name: "{autoprograms}\Kudu"; Filename: "{app}\kudu.exe"
Name: "{autodesktop}\Kudu"; Filename: "{app}\kudu.exe"; Tasks: desktopicon

[Tasks]
Name: "desktopicon"; Description: "Create a desktop shortcut"; Flags: unchecked

[Run]
Filename: "{app}\kudu.exe"; Description: "Start Kudu"; Flags: nowait postinstall skipifsilent

[UninstallRun]
; Stop Kudu (and its window) before removing it.
Filename: "{sys}\taskkill.exe"; Parameters: "/F /IM kudu.exe"; Flags: runhidden; RunOnceId: "StopKudu"

[Registry]
; Remove the "start at login" entry Kudu may have added.
Root: HKCU; Subkey: "Software\Microsoft\Windows\CurrentVersion\Run"; ValueName: "Kudu"; Flags: uninsdeletevalue dontcreatekey
