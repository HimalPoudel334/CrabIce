; --------------------------------------------------------------
; CrabIce.iss — Full installer with MinGW + GStreamer (media)
; --------------------------------------------------------------

#define MyAppName      "CrabIce"
#define MyAppVersion   "0.1.0"
#define MyAppPublisher "Himal Poudel"
#define MyAppURL       "https://github.com/himalpoudel334/CrabIce"
#define MyAppExeName   "CrabIce.exe"

#define MyAppSourceDir "C:\Users\himal\Documents\Projects\rust\CrabIce\target\release"
#define MSYS2_MINGW64  "C:\msys64\mingw64"

[Setup]
AppId={{B9E5F7A1-2C3D-4E5F-9A1B-7C8D9E0F1A2B}}
AppName={#MyAppName}
AppVersion={#MyAppVersion}
AppPublisher={#MyAppPublisher}
AppPublisherURL={#MyAppURL}
DefaultDirName={autopf}\{#MyAppName}
DefaultGroupName={#MyAppName}
OutputDir=Output
OutputBaseFilename=CrabIce_Setup_v{#MyAppVersion}
Compression=lzma
SolidCompression=yes
WizardStyle=modern
ArchitecturesInstallIn64BitMode=x64
PrivilegesRequired=admin
UninstallDisplayIcon={app}\CrabIce.ico

[Languages]
Name: "english"; MessagesFile: "compiler:Default.isl"

[Tasks]
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"; Flags: unchecked

; --------------------------------------------------------------
; FILES
; --------------------------------------------------------------
[Files]
; Main executable
Source: "{#MyAppSourceDir}\{#MyAppExeName}"; DestDir: "{app}"; Flags: ignoreversion

; Icon
Source: "CrabIce.ico"; DestDir: "{app}"; Flags: ignoreversion

; -------------------------
; MinGW runtime
; -------------------------
Source: "{#MSYS2_MINGW64}\bin\libgcc_s_seh-1.dll"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#MSYS2_MINGW64}\bin\libwinpthread-1.dll"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#MSYS2_MINGW64}\bin\libstdc++-6.dll"; DestDir: "{app}"; Flags: ignoreversion

; -------------------------
; GStreamer core
; -------------------------
Source: "{#MSYS2_MINGW64}\bin\libgstreamer-1.0-0.dll"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#MSYS2_MINGW64}\bin\libgstbase-1.0-0.dll"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#MSYS2_MINGW64}\bin\libgstvideo-1.0-0.dll"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#MSYS2_MINGW64}\bin\libgstapp-1.0-0.dll"; DestDir: "{app}"; Flags: ignoreversion

; -------------------------
; GLib stack
; -------------------------
Source: "{#MSYS2_MINGW64}\bin\libglib-2.0-0.dll"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#MSYS2_MINGW64}\bin\libgobject-2.0-0.dll"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#MSYS2_MINGW64}\bin\libgmodule-2.0-0.dll"; DestDir: "{app}"; Flags: ignoreversion

; -------------------------
; Supporting libraries
; -------------------------
Source: "{#MSYS2_MINGW64}\bin\liborc-0.4-0.dll"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#MSYS2_MINGW64}\bin\libintl-8.dll"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#MSYS2_MINGW64}\bin\libiconv-2.dll"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#MSYS2_MINGW64}\bin\libffi-8.dll"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#MSYS2_MINGW64}\bin\libpcre2-8-0.dll"; DestDir: "{app}"; Flags: ignoreversion

; -------------------------
; GStreamer plugins (MEDIA)
; -------------------------
Source: "{#MSYS2_MINGW64}\lib\gstreamer-1.0\*"; DestDir: "{app}\gstreamer-1.0"; Flags: recursesubdirs ignoreversion

; --------------------------------------------------------------
; ICONS
; --------------------------------------------------------------
[Icons]
Name: "{group}\{#MyAppName}"; Filename: "{app}\{#MyAppExeName}"; IconFilename: "{app}\CrabIce.ico"
Name: "{autodesktop}\{#MyAppName}"; Filename: "{app}\{#MyAppExeName}"; Tasks: desktopicon; IconFilename: "{app}\CrabIce.ico"
Name: "{group}\{cm:UninstallProgram,{#MyAppName}}"; Filename: "{uninstallexe}"

; --------------------------------------------------------------
; RUN
; --------------------------------------------------------------
[Run]
Filename: "{cmd}"; Parameters: "/C setx GST_PLUGIN_PATH ""{app}\gstreamer-1.0"""; Flags: runhidden
Filename: "{app}\{#MyAppExeName}"; Description: "{cm:LaunchProgram,{#MyAppName}}"; Flags: nowait postinstall skipifsilent
