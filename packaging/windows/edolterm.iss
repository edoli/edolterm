#ifndef PayloadDir
  #define PayloadDir "..\..\dist\windows\payload"
#endif
#ifndef AppVersion
  #error AppVersion must come from Cargo.toml
#endif
#ifndef AppIdentity
  #define AppIdentity "{8164B90B-884C-40B0-AE1D-83B392FB1015}"
#endif
#ifndef AppDisplayName
  #define AppDisplayName "Edolterm"
#endif
[Setup]
AppId={#StringChange(AppIdentity, "{", "{{")}
AppName={#AppDisplayName}
AppVersion={#AppVersion}
AppPublisher=Edoli
DefaultDirName={autopf}\Edolterm
DefaultGroupName={#AppDisplayName}
PrivilegesRequired=lowest
PrivilegesRequiredOverridesAllowed=dialog
UsePreviousPrivileges=yes
UsePreviousTasks=yes
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
MinVersion=10.0.17763
OutputBaseFilename=edolterm-installer-exe
Compression=lzma2
SolidCompression=yes
WizardStyle=modern
UninstallDisplayIcon={app}\edolterm.exe
CloseApplications=no
SetupLogging=yes
RestartApplications=no

[Languages]
Name: "korean"; MessagesFile: "compiler:Languages\Korean.isl"
Name: "english"; MessagesFile: "compiler:Default.isl"

[Tasks]
; Preserve the previous selection during silent updates so removed integration is reinstalled.
Name: "explorer"; Description: "탐색기 메뉴 및 Windows 기본 터미널 연동 설치 (인증서 등록 시 관리자 승인 필요)"
Name: "desktopicon"; Description: "바탕 화면 바로 가기 만들기"; Flags: unchecked

[Files]
Source: "{#PayloadDir}\edolterm.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#PayloadDir}\shell-integration\Assets\*.png"; DestDir: "{app}\shell-integration\Assets"; Flags: ignoreversion
Source: "{#PayloadDir}\shell-integration\resources.pri"; DestDir: "{app}\shell-integration"; Flags: ignoreversion
Source: "{#PayloadDir}\conpty.dll"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#PayloadDir}\OpenConsole.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#PayloadDir}\LICENSE-ConPTY.txt"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#PayloadDir}\shell-integration\Edolterm.ExplorerCommand.dll"; DestDir: "{app}\shell-integration"; Flags: ignoreversion
Source: "{#PayloadDir}\shell-integration\Edolterm.ExplorerHost.exe"; DestDir: "{app}\shell-integration"; Flags: ignoreversion
Source: "{#PayloadDir}\shell-integration\Edolterm.TerminalHost.dll"; DestDir: "{app}\shell-integration"; Flags: ignoreversion
Source: "{#PayloadDir}\shell-integration\Edolterm.OpenConsole.exe"; DestDir: "{app}\shell-integration"; Flags: ignoreversion
Source: "{#PayloadDir}\shell-integration\openconsole-build.json"; DestDir: "{app}\shell-integration"; Flags: ignoreversion
Source: "{#PayloadDir}\shell-integration\Edolterm.HandoffProxy.dll"; DestDir: "{app}\shell-integration"; Flags: ignoreversion
Source: "{#PayloadDir}\shell-integration\LICENSE-MicrosoftTerminal.txt"; DestDir: "{app}\shell-integration"; Flags: ignoreversion
Source: "{#PayloadDir}\shell-integration\NOTICE-OpenConsole.txt"; DestDir: "{app}\shell-integration"; Flags: ignoreversion
Source: "{#PayloadDir}\shell-integration\windows-default-terminal.ps1"; DestDir: "{app}\shell-integration"; Flags: ignoreversion
Source: "{#PayloadDir}\shell-integration\windows-terminal-support.ps1"; DestDir: "{app}\shell-integration"; Flags: ignoreversion
Source: "{#PayloadDir}\shell-integration\Edolterm.Explorer.msix"; DestDir: "{app}\shell-integration"; Flags: ignoreversion
Source: "{#PayloadDir}\shell-integration\Edolterm.Development.cer"; DestDir: "{app}\shell-integration"; Flags: ignoreversion
Source: "{#PayloadDir}\shell-integration\windows-shell-integration.ps1"; DestDir: "{app}\shell-integration"; Flags: ignoreversion

Source: "{#PayloadDir}\explorer-context-menu.ps1"; DestDir: "{app}"; Flags: ignoreversion

[Registry]
Root: HKA; Subkey: "Software\Edoli\{#AppDisplayName}"; ValueType: string; ValueName: "InstallLocation"; ValueData: "{app}"; Flags: uninsdeletekey

[Icons]
Name: "{group}\Edolterm"; Filename: "{app}\edolterm.exe"; AppUserModelID: "dev.edoli.edolterm"
Name: "{autodesktop}\Edolterm"; Filename: "{app}\edolterm.exe"; AppUserModelID: "dev.edoli.edolterm"; Tasks: desktopicon

[Run]
Filename: "{app}\edolterm.exe"; Description: "Edolterm 실행"; Flags: nowait postinstall skipifsilent unchecked runasoriginaluser

[UninstallDelete]
Type: files; Name: "{app}\explorer-*.log"
Type: files; Name: "{app}\explorer-integration.enabled"

[Code]
var
  IntegrationFailed: Boolean;

function CreateFileW(lpFileName: String; dwDesiredAccess, dwShareMode, lpSecurityAttributes,
  dwCreationDisposition, dwFlagsAndAttributes: LongWord; hTemplateFile: THandle): THandle;
  external 'CreateFileW@kernel32.dll stdcall';
function CloseHandle(hObject: THandle): Boolean;
  external 'CloseHandle@kernel32.dll stdcall';

function RunShellIntegration(Action: String; PreserveDefault: Boolean): Boolean;
var
  ExitCode: Integer;
  Parameters, Scope, IntegrationDirectory: String;
begin
  if IsAdminInstallMode then Scope := 'Machine' else Scope := 'User';
  IntegrationDirectory := ExpandConstant('{app}\shell-integration');
  { Read the old helper when upgrading a release with a flat installation layout. }
  if not FileExists(IntegrationDirectory + '\windows-shell-integration.ps1') then
    IntegrationDirectory := ExpandConstant('{app}');
  Parameters := '-NoLogo -NoProfile -NonInteractive -ExecutionPolicy Bypass -File ' +
    AddQuotes(IntegrationDirectory + '\windows-shell-integration.ps1') +
    ' -Action ' + Action + ' -AppDirectory ' + AddQuotes(IntegrationDirectory) + ' -Scope ' + Scope;
  { Older integration helpers predate this option and cannot accept it. }
  if PreserveDefault and FileExists(IntegrationDirectory + '\windows-default-terminal.ps1') then
    Parameters := Parameters + ' -PreserveDefaultTerminal';
  Result := Exec(ExpandConstant('{sys}\WindowsPowerShell\v1.0\powershell.exe'),
    Parameters, '', SW_HIDE, ewWaitUntilTerminated, ExitCode);
  Log('Explorer integration ' + Action + ': launched=' + IntToStr(Ord(Result)) + ', exit=' + IntToStr(ExitCode));
  Result := Result and (ExitCode = 0);
end;

function ExecutableAvailable(): Boolean;
var
  Handle: THandle;
  Path: String;
begin
  Path := ExpandConstant('{app}\edolterm.exe');
  Result := True;
  if not FileExists(Path) then Exit;
  Handle := CreateFileW(Path, $40000000, 7, 0, 3, $80, 0);
  Result := Handle <> THandle(-1);
  if Result then CloseHandle(Handle);
end;

function PrepareToInstall(var NeedsRestart: Boolean): String;
begin
  Result := '';
  if not ExecutableAvailable() then
  begin
    Result := 'Edolterm이 실행 중입니다. 모든 창을 직접 닫은 뒤 다시 설치하세요.';
    Exit;
  end;
  if FileExists(ExpandConstant('{app}\explorer-integration.enabled')) or
    (FileExists(ExpandConstant('{app}\windows-shell-integration.ps1')) and
     not FileExists(ExpandConstant('{app}\explorer-context-menu.ps1'))) then
    if not RunShellIntegration('Uninstall', WizardIsTaskSelected('explorer')) then
      Result := '기존 탐색기 메뉴 등록을 해제하지 못했습니다. Edolterm을 종료한 뒤 다시 시도하세요.';
  if Result = '' then DeleteFile(ExpandConstant('{app}\explorer-integration.enabled'));
end;

procedure CurStepChanged(CurStep: TSetupStep);
begin
  if (CurStep = ssPostInstall) and WizardIsTaskSelected('explorer') then
  begin
    SaveStringToFile(ExpandConstant('{app}\explorer-integration.enabled'), 'enabled', False);
    if not RunShellIntegration('Install', False) then
    begin
      IntegrationFailed := True;
      SuppressibleMsgBox('Edolterm 메뉴 등록에 실패했습니다. 인증서 등록의 관리자 승인을 허용했는지 확인하세요.' + #13#10 +
        ExpandConstant('{localappdata}\Edolterm\Logs\explorer-Install.log'), mbError, MB_OK, IDOK);
    end;
  end;
end;

function GetCustomSetupExitCode(): Integer;
begin
  if IntegrationFailed then Result := 1 else Result := 0;
end;

function InitializeUninstall(): Boolean;
begin
  Result := ExecutableAvailable();
  if not Result then
  begin
    MsgBox('Edolterm이 실행 중입니다. 모든 창을 직접 닫은 뒤 제거하세요.', mbError, MB_OK);
    Exit;
  end;
  if FileExists(ExpandConstant('{app}\explorer-integration.enabled')) then
    if not RunShellIntegration('Uninstall', False) then
    begin
      MsgBox('탐색기 메뉴 등록을 해제하지 못했습니다. 실행 중인 Edolterm을 종료한 뒤 다시 시도하세요.', mbError, MB_OK);
      Result := False;
    end;
end;
