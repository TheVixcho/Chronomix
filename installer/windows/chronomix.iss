#ifndef MyAppVersion
  #define MyAppVersion "0.0.1"
#endif

[Setup]
AppId={{8E9B7D4A-3C21-4F8E-9A15-6D2C8E4B1092}
AppName=Chronomix
AppVersion={#MyAppVersion}
AppVerName=Chronomix {#MyAppVersion}
AppPublisher=TheVixcho
AppPublisherURL=https://github.com/TheVixcho/chronomix
AppSupportURL=https://github.com/TheVixcho/chronomix/issues
AppUpdatesURL=https://github.com/TheVixcho/chronomix/releases
DefaultDirName={autopf}\Chronomix
DefaultGroupName=Chronomix
AllowNoIcons=yes
LicenseFile=..\..\LICENSE
OutputDir=..\..\dist
OutputBaseFilename=Chronomix-Setup-v{#MyAppVersion}-Windows-x64
Compression=lzma2
SolidCompression=yes
WizardStyle=modern
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
PrivilegesRequiredOverridesAllowed=dialog

[Languages]
Name: "spanish"; MessagesFile: "compiler:Languages\Spanish.isl"
Name: "english"; MessagesFile: "compiler:Default.isl"

[Tasks]
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"

[Dirs]
Name: "{app}\vst_plugins"

[Files]
Source: "..\..\target\release\chronomix.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\..\LICENSE"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\..\README.md"; DestDir: "{app}"; Flags: ignoreversion

[Icons]
Name: "{group}\Chronomix"; Filename: "{app}\chronomix.exe"
Name: "{group}\{cm:UninstallProgram,Chronomix}"; Filename: "{uninstallexe}"
Name: "{autodesktop}\Chronomix"; Filename: "{app}\chronomix.exe"; Tasks: desktopicon

[Run]
Filename: "{app}\chronomix.exe"; Description: "{cm:LaunchProgram,Chronomix}"; Flags: nowait postinstall skipifsilent
