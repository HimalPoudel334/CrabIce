; --------------------------------------------------------------
; crabipiesetup.iss — FULL MULTIMEDIA PRODUCTION REBUILD
; --------------------------------------------------------------

#define MyAppName      "CrabiPie"
#define MyAppVersion   "0.1.0"
#define MyAppPublisher "Himal Poudel"
#define MyAppURL       "https://github.com"
#define MyAppExeName   "CrabiPie.exe"

#define MyAppSourceDir "C:\Users\HP\Documents\Projects\rust\crabice\target\release"
#define GST_MSVC_DIR   "C:\Program Files\gstreamer\1.0\msvc_x86_64"

[Setup]
AppId={{B9E5F7A1-2C3D-4E5F-9A1B-7C8D9E0F1A2B}}
AppName={#MyAppName}
AppVersion={#MyAppVersion}
AppPublisher={#MyAppPublisher}
AppPublisherURL={#MyAppURL}
DefaultDirName={autopf}\{#MyAppName}
DefaultGroupName={#MyAppName}
OutputDir=Output
OutputBaseFilename=CrabiPie_Setup_v{#MyAppVersion}
; SPEED SETTING: High-speed multi-threaded configuration for rapid testing
Compression=lzma/fast
SolidCompression=yes
WizardStyle=modern
ArchitecturesInstallIn64BitMode=x64compatible
PrivilegesRequired=admin
UninstallDisplayIcon={app}\CrabiPie.ico

[Languages]
Name: "english"; MessagesFile: "compiler:Default.isl"

[Tasks]
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"; Flags: unchecked

; --------------------------------------------------------------
; FILES (Captures your entire previous video/audio playback matrix)
; --------------------------------------------------------------
[Files]
; Main application binaries
Source: "{#MyAppSourceDir}\{#MyAppExeName}"; DestDir: "{app}"; Flags: ignoreversion
Source: "CrabiPie.ico"; DestDir: "{app}"; Flags: ignoreversion

; --- Foundational Core Helpers ---
Source: "{#GST_MSVC_DIR}\bin\intl-8.dll"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#GST_MSVC_DIR}\bin\iconv-2.dll"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#GST_MSVC_DIR}\bin\orc-0.4-0.dll"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#GST_MSVC_DIR}\bin\pcre2-8*.dll"; DestDir: "{app}"; Flags: ignoreversion

; --- Core Engine Managers (Wildcards target root bin directories instantly) ---
Source: "{#GST_MSVC_DIR}\bin\g*.dll"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#GST_MSVC_DIR}\bin\f*.dll"; DestDir: "{app}"; Flags: ignoreversion

; --- Complete Codec Library (Filters out slow developer metadata automatically) ---
; This line cleanly packages flac, matroska, jpeg, mp4, hls, and display sinks in ~10 seconds!
Source: "{#GST_MSVC_DIR}\lib\gstreamer-1.0\*.dll"; DestDir: "{app}\gstreamer-1.0"; Flags: ignoreversion

; --------------------------------------------------------------
; RUN-TIME CODES
; --------------------------------------------------------------
[Icons]
Name: "{group}\{#MyAppName}"; Filename: "{app}\{#MyAppExeName}"; IconFilename: "{app}\CrabiPie.ico"
Name: "{autodesktop}\{#MyAppName}"; Filename: "{app}\{#MyAppExeName}"; Tasks: desktopicon; IconFilename: "{app}\CrabiPie.ico"
Name: "{group}\{cm:UninstallProgram,{#MyAppName}}"; Filename: "{uninstallexe}"

[Run]
Filename: "{cmd}"; Parameters: "/C setx GST_PLUGIN_PATH ""{app}\gstreamer-1.0"""; Flags: runhidden
Filename: "{app}\{#MyAppExeName}"; Description: "{cm:LaunchProgram,{#MyAppName}}"; Flags: nowait postinstall skipifsilent

[Code]
procedure CurUninstallStepChanged(CurUninstallStep: TUninstallStep);
var
  StatePath: String;
begin
  if CurUninstallStep = usPostUninstall then
  begin
    StatePath := ExpandConstant('{%USERPROFILE}\.crabipie');
    if DirExists(StatePath) then
    begin
      if MsgBox('Remove CrabiPie saved data (sessions, collections, cookies)?'#13#10 +
                StatePath, mbConfirmation, MB_YESNO) = IDYES then
      begin
        DelTree(StatePath, True, True, True);
      end;
    end;
  end;
end;
