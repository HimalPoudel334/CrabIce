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
; FILES
; --------------------------------------------------------------
[Files]
; ── Main application ───────────────────────────────────────────
Source: "{#MyAppSourceDir}\{#MyAppExeName}"; DestDir: "{app}"; Flags: ignoreversion
Source: "CrabiPie.ico";                      DestDir: "{app}"; Flags: ignoreversion

; ── Core GLib runtime ──────────────────────────────────────────
Source: "{#GST_MSVC_DIR}\bin\glib-2.0-0.dll";       DestDir: "{app}"; Flags: ignoreversion
Source: "{#GST_MSVC_DIR}\bin\gobject-2.0-0.dll";    DestDir: "{app}"; Flags: ignoreversion
Source: "{#GST_MSVC_DIR}\bin\gmodule-2.0-0.dll";    DestDir: "{app}"; Flags: ignoreversion
Source: "{#GST_MSVC_DIR}\bin\gthread-2.0-0.dll";    DestDir: "{app}"; Flags: ignoreversion
Source: "{#GST_MSVC_DIR}\bin\gio-2.0-0.dll";        DestDir: "{app}"; Flags: ignoreversion
Source: "{#GST_MSVC_DIR}\bin\intl-8.dll";            DestDir: "{app}"; Flags: ignoreversion
Source: "{#GST_MSVC_DIR}\bin\iconv-2.dll";           DestDir: "{app}"; Flags: ignoreversion
Source: "{#GST_MSVC_DIR}\bin\ffi-7.dll";             DestDir: "{app}"; Flags: ignoreversion
Source: "{#GST_MSVC_DIR}\bin\orc-0.4-0.dll";         DestDir: "{app}"; Flags: ignoreversion
Source: "{#GST_MSVC_DIR}\bin\pcre2-8*.dll";          DestDir: "{app}"; Flags: ignoreversion

; ── GStreamer runtime ───────────────────────────────────────────
Source: "{#GST_MSVC_DIR}\bin\gstreamer-1.0-0.dll";  DestDir: "{app}"; Flags: ignoreversion
Source: "{#GST_MSVC_DIR}\bin\gstbase-1.0-0.dll";    DestDir: "{app}"; Flags: ignoreversion
Source: "{#GST_MSVC_DIR}\bin\gstvideo-1.0-0.dll";   DestDir: "{app}"; Flags: ignoreversion
Source: "{#GST_MSVC_DIR}\bin\gstaudio-1.0-0.dll";   DestDir: "{app}"; Flags: ignoreversion
Source: "{#GST_MSVC_DIR}\bin\gstpbutils-1.0-0.dll"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#GST_MSVC_DIR}\bin\gstapp-1.0-0.dll";     DestDir: "{app}"; Flags: ignoreversion

; ── libsoup (HTTP source) + dependencies ───────────────────────
Source: "{#GST_MSVC_DIR}\bin\soup-3.0-0.dll";       DestDir: "{app}"; Flags: ignoreversion
Source: "{#GST_MSVC_DIR}\bin\nghttp2.dll";           DestDir: "{app}"; Flags: ignoreversion
Source: "{#GST_MSVC_DIR}\bin\psl-5.dll";             DestDir: "{app}"; Flags: ignoreversion
Source: "{#GST_MSVC_DIR}\bin\sqlite3-0.dll";         DestDir: "{app}"; Flags: ignoreversion
Source: "{#GST_MSVC_DIR}\bin\proxy-1.dll";           DestDir: "{app}"; Flags: ignoreversion

; ── gio modules ────────────────────────────────────────────────
Source: "{#GST_MSVC_DIR}\lib\gio\modules\giolibproxy.dll"; DestDir: "{app}\gio\modules"; Flags: ignoreversion

; ── ffmpeg (H.264 software decode) ─────────────────────────────
Source: "{#GST_MSVC_DIR}\bin\avcodec-60.dll";        DestDir: "{app}"; Flags: ignoreversion skipifsourcedoesntexist
Source: "{#GST_MSVC_DIR}\bin\avformat-60.dll";       DestDir: "{app}"; Flags: ignoreversion skipifsourcedoesntexist
Source: "{#GST_MSVC_DIR}\bin\avutil-58.dll";         DestDir: "{app}"; Flags: ignoreversion skipifsourcedoesntexist
Source: "{#GST_MSVC_DIR}\bin\swresample-4.dll";      DestDir: "{app}"; Flags: ignoreversion skipifsourcedoesntexist
Source: "{#GST_MSVC_DIR}\bin\swscale-7.dll";         DestDir: "{app}"; Flags: ignoreversion skipifsourcedoesntexist

; ── GStreamer plugins ───────────────────────────────────────────
Source: "{#GST_MSVC_DIR}\lib\gstreamer-1.0\gstcoreelements.dll";      DestDir: "{app}\gstreamer-1.0"; Flags: ignoreversion
Source: "{#GST_MSVC_DIR}\lib\gstreamer-1.0\gstapp.dll";               DestDir: "{app}\gstreamer-1.0"; Flags: ignoreversion
Source: "{#GST_MSVC_DIR}\lib\gstreamer-1.0\gstplayback.dll";          DestDir: "{app}\gstreamer-1.0"; Flags: ignoreversion
Source: "{#GST_MSVC_DIR}\lib\gstreamer-1.0\gsttypefindfunctions.dll"; DestDir: "{app}\gstreamer-1.0"; Flags: ignoreversion
Source: "{#GST_MSVC_DIR}\lib\gstreamer-1.0\gstsoup.dll";              DestDir: "{app}\gstreamer-1.0"; Flags: ignoreversion
Source: "{#GST_MSVC_DIR}\lib\gstreamer-1.0\gstisomp4.dll";            DestDir: "{app}\gstreamer-1.0"; Flags: ignoreversion
Source: "{#GST_MSVC_DIR}\lib\gstreamer-1.0\gstmatroska.dll";          DestDir: "{app}\gstreamer-1.0"; Flags: ignoreversion
Source: "{#GST_MSVC_DIR}\lib\gstreamer-1.0\gstvideoparsersbad.dll";   DestDir: "{app}\gstreamer-1.0"; Flags: ignoreversion
Source: "{#GST_MSVC_DIR}\lib\gstreamer-1.0\gstvideoconvertscale.dll"; DestDir: "{app}\gstreamer-1.0"; Flags: ignoreversion
Source: "{#GST_MSVC_DIR}\lib\gstreamer-1.0\gstvideofilter.dll";       DestDir: "{app}\gstreamer-1.0"; Flags: ignoreversion
Source: "{#GST_MSVC_DIR}\lib\gstreamer-1.0\gstsubparse.dll";          DestDir: "{app}\gstreamer-1.0"; Flags: ignoreversion
Source: "{#GST_MSVC_DIR}\lib\gstreamer-1.0\gstaudioconvert.dll";      DestDir: "{app}\gstreamer-1.0"; Flags: ignoreversion
Source: "{#GST_MSVC_DIR}\lib\gstreamer-1.0\gstaudioresample.dll";     DestDir: "{app}\gstreamer-1.0"; Flags: ignoreversion
Source: "{#GST_MSVC_DIR}\lib\gstreamer-1.0\gstlibav.dll";             DestDir: "{app}\gstreamer-1.0"; Flags: ignoreversion skipifsourcedoesntexist
Source: "{#GST_MSVC_DIR}\lib\gstreamer-1.0\gstd3d11.dll";             DestDir: "{app}\gstreamer-1.0"; Flags: ignoreversion skipifsourcedoesntexist
Source: "{#GST_MSVC_DIR}\lib\gstreamer-1.0\gstwasapi2.dll";           DestDir: "{app}\gstreamer-1.0"; Flags: ignoreversion skipifsourcedoesntexist
Source: "{#GST_MSVC_DIR}\lib\gstreamer-1.0\gstwasapi.dll";            DestDir: "{app}\gstreamer-1.0"; Flags: ignoreversion skipifsourcedoesntexist
Source: "{#GST_MSVC_DIR}\lib\gstreamer-1.0\gstautodetect.dll";        DestDir: "{app}\gstreamer-1.0"; Flags: ignoreversion

; --------------------------------------------------------------
; ICONS
; --------------------------------------------------------------
[Icons]
Name: "{group}\{#MyAppName}";                    Filename: "{app}\{#MyAppExeName}"; IconFilename: "{app}\CrabiPie.ico"
Name: "{autodesktop}\{#MyAppName}";              Filename: "{app}\{#MyAppExeName}"; Tasks: desktopicon; IconFilename: "{app}\CrabiPie.ico"
Name: "{group}\{cm:UninstallProgram,{#MyAppName}}"; Filename: "{uninstallexe}"

; --------------------------------------------------------------
; REGISTRY — persistent environment variables (replaces setx)
; --------------------------------------------------------------
[Registry]
Root: HKCU; Subkey: "Environment"; ValueType: string; ValueName: "GST_PLUGIN_PATH"; ValueData: "{app}\gstreamer-1.0"; Flags: uninsdeletevalue
Root: HKCU; Subkey: "Environment"; ValueType: string; ValueName: "GIO_MODULE_DIR";  ValueData: "{app}\gio\modules";    Flags: uninsdeletevalue

; --------------------------------------------------------------
; RUN
; --------------------------------------------------------------
[Run]
Filename: "{app}\{#MyAppExeName}"; Description: "{cm:LaunchProgram,{#MyAppName}}"; Flags: nowait postinstall skipifsilent

; --------------------------------------------------------------
; UNINSTALL — clean up saved state
; --------------------------------------------------------------
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