; Turks installer (Inno Setup 6). Built by GitHub Actions:
;   iscc /DMyAppVersion=x.y.z installer\turks.iss

#ifndef MyAppVersion
  #define MyAppVersion "0.0.0"
#endif
#define MyAppName "Turks"
#define MyAppExe "turks.exe"
#define MyAppPublisher "AnimaTheAeon"
#define MyAppURL "https://github.com/AnimaTheAeon/Turks"

[Setup]
AppId={{3B8E1C52-7D49-4A1F-B2E6-9C0D4F7A6E13}
AppName={#MyAppName}
AppVersion={#MyAppVersion}
AppVerName={#MyAppName} {#MyAppVersion}
AppPublisher={#MyAppPublisher}
AppPublisherURL={#MyAppURL}
AppSupportURL={#MyAppURL}/issues
AppUpdatesURL={#MyAppURL}/releases
; Per-user install: no admin prompt, lands in %LOCALAPPDATA%\Programs\Turks.
PrivilegesRequired=lowest
DefaultDirName={autopf}\{#MyAppName}
DefaultGroupName={#MyAppName}
DisableProgramGroupPage=yes
UninstallDisplayIcon={app}\{#MyAppExe}
UninstallDisplayName={#MyAppName}
SetupIconFile=..\resources\turks.ico
WizardStyle=modern
WizardSmallImageFile=wizard-small.bmp
WizardImageFile=wizard-large.bmp
OutputDir=..\target\installer
OutputBaseFilename=Turks-Setup-{#MyAppVersion}
Compression=lzma2/max
SolidCompression=yes
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
; Setup closes a running Turks itself (see CloseTurks below) instead of
; stopping to ask, so an install never waits on a copy hidden in the tray.
CloseApplications=force
RestartApplications=no
LicenseFile=..\LICENSE

[Languages]
Name: "english"; MessagesFile: "compiler:Default.isl"

[Tasks]
Name: "desktopicon"; Description: "Create a &desktop shortcut"; GroupDescription: "Shortcuts:"
Name: "autostart"; Description: "Start Turks with &Windows (in the tray)"; GroupDescription: "Integration:"

[Files]
Source: "..\target\release\{#MyAppExe}"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\README.md"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\LICENSE"; DestDir: "{app}"; Flags: ignoreversion

[Icons]
Name: "{autoprograms}\{#MyAppName}"; Filename: "{app}\{#MyAppExe}"
Name: "{autodesktop}\{#MyAppName}"; Filename: "{app}\{#MyAppExe}"; Tasks: desktopicon

[Registry]
Root: HKCU; Subkey: "Software\Microsoft\Windows\CurrentVersion\Run"; ValueType: string; ValueName: "Turks"; ValueData: """{app}\{#MyAppExe}"" --tray"; Tasks: autostart
Root: HKCU; Subkey: "Software\Microsoft\Windows\CurrentVersion\Run"; ValueType: none; ValueName: "Turks"; Flags: uninsdeletevalue

[Run]
Filename: "{app}\{#MyAppExe}"; Description: "Launch Turks"; Flags: nowait postinstall skipifsilent
Filename: "{app}\{#MyAppExe}"; Parameters: "--after-update"; Flags: nowait; Check: WizardSilent

[Code]
const
  TurksMutex = 'TurksSingleInstance';

function TurksGone(Steps: Integer): Boolean;
var
  I: Integer;
begin
  for I := 1 to Steps do
  begin
    if not CheckForMutexes(TurksMutex) then
    begin
      Result := True;
      Exit;
    end;
    Sleep(250);
  end;
  Result := not CheckForMutexes(TurksMutex);
end;

{ Ask a running Turks to save and quit; force it after 10 seconds. }
function CloseTurks(): Boolean;
var
  DataDir: String;
  ResultCode: Integer;
begin
  Result := True;
  if not CheckForMutexes(TurksMutex) then
    Exit;
  DataDir := ExpandConstant('{userappdata}\Turks');
  ForceDirectories(DataDir);
  SaveStringToFile(DataDir + '\quit.request', '1', False);
  if not TurksGone(40) then
  begin
    Exec(ExpandConstant('{sys}\taskkill.exe'), '/F /IM {#MyAppExe}', '', SW_HIDE, ewWaitUntilTerminated, ResultCode);
    Result := TurksGone(40);
  end;
  DeleteFile(DataDir + '\quit.request');
end;

function PrepareToInstall(var NeedsRestart: Boolean): String;
begin
  Result := '';
  if not CloseTurks() then
    Result := 'Turks is still running and could not be closed. Quit it from its tray icon, then click Back and Next.';
end;

function InitializeUninstall(): Boolean;
begin
  Result := CloseTurks();
  if not Result and not UninstallSilent then
    MsgBox('Turks is still running and could not be closed. Quit it from its tray icon and run the uninstaller again.', mbError, MB_OK);
end;

procedure CurUninstallStepChanged(CurUninstallStep: TUninstallStep);
var
  DataDir: String;
begin
  if CurUninstallStep = usPostUninstall then
  begin
    DataDir := ExpandConstant('{userappdata}\Turks');
    if DirExists(DataDir) and not UninstallSilent then
      if MsgBox('Also delete Turks'' library database, settings, backups and cached posters?' + #13#10 + #13#10 +
                'Your movie files are not touched either way.',
                mbConfirmation, MB_YESNO or MB_DEFBUTTON2) = IDYES then
        DelTree(DataDir, True, True, True);
  end;
end;

procedure CurStepChanged(CurStep: TSetupStep);
begin
  if (CurStep = ssPostInstall) and WizardIsTaskSelected('autostart') then
  begin
    ForceDirectories(ExpandConstant('{userappdata}\Turks'));
    SaveStringToFile(ExpandConstant('{userappdata}\Turks\installer-autostart'), '1', False);
  end;
end;
