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
ShowLanguageDialog=no
LanguageDetectionMethod=uilanguage
UsePreviousLanguage=no

[Languages]
Name: "english"; MessagesFile: "compiler:Default.isl"
Name: "korean"; MessagesFile: "compiler:Languages\Korean.isl"

[CustomMessages]
english.ExplorerTask=Add Edolterm to the Explorer context menu
korean.ExplorerTask=탐색기 메뉴에 Edolterm 등록
english.DefaultTerminalTask=Set Edolterm as the current user's default terminal
korean.DefaultTerminalTask=Edolterm을 현재 사용자의 기본 터미널로 설정
english.DesktopTask=Create a desktop shortcut
korean.DesktopTask=바탕 화면 바로 가기 만들기
english.LaunchApp=Launch Edolterm
korean.LaunchApp=Edolterm 실행
english.UnsupportedTerminal=Setting the default terminal requires Windows 11 or an updated Windows 10 22H2.
korean.UnsupportedTerminal=기본 터미널 설정에는 Windows 11 또는 업데이트된 Windows 10 22H2가 필요합니다.
english.CloseBeforeInstall=Edolterm is running. Close all windows before installing again.
korean.CloseBeforeInstall=Edolterm이 실행 중입니다. 모든 창을 직접 닫은 뒤 다시 설치하세요.
english.RemoveOldIntegrationFailed=Failed to remove the previous Windows integration. Close Edolterm and try again.
korean.RemoveOldIntegrationFailed=기존 Windows 연동을 해제하지 못했습니다. Edolterm을 종료한 뒤 다시 시도하세요.
english.IntegrationFailed=Edolterm Windows integration failed. Check that you allowed administrator approval for certificate registration.
korean.IntegrationFailed=Edolterm Windows 연동에 실패했습니다. 인증서 등록의 관리자 승인을 허용했는지 확인하세요.
english.DefaultTerminalFailed=Failed to set the current user's default terminal. Run Setup again or apply it in the app settings.
korean.DefaultTerminalFailed=현재 사용자의 기본 터미널 설정에 실패했습니다. 설치를 다시 실행하거나 앱 설정에서 적용하세요.
english.CloseBeforeUninstall=Edolterm is running. Close all windows before uninstalling.
korean.CloseBeforeUninstall=Edolterm이 실행 중입니다. 모든 창을 직접 닫은 뒤 제거하세요.
english.RemoveIntegrationFailed=Failed to remove Windows integration. Close all running Edolterm windows and try again.
korean.RemoveIntegrationFailed=Windows 연동을 해제하지 못했습니다. 실행 중인 Edolterm을 종료한 뒤 다시 시도하세요.

[Tasks]
; Preserve the previous selection during silent updates so removed integration is reinstalled.
Name: "explorer"; Description: "{cm:ExplorerTask}"
Name: "defaultterminal"; Description: "{cm:DefaultTerminalTask}"; Flags: unchecked; Check: DefaultTerminalSupported
Name: "desktopicon"; Description: "{cm:DesktopTask}"; Flags: unchecked

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
Filename: "{app}\edolterm.exe"; Description: "{cm:LaunchApp}"; Flags: nowait postinstall skipifsilent unchecked runasoriginaluser

[UninstallDelete]
Type: files; Name: "{app}\explorer-*.log"
Type: files; Name: "{app}\explorer-integration.enabled"
Type: files; Name: "{app}\terminal-integration.enabled"
Type: files; Name: "{app}\windows-integration.enabled"

[Code]
var
  IntegrationFailed: Boolean;
  IntegrationUpgrade: Boolean;

function CreateFileW(lpFileName: String; dwDesiredAccess, dwShareMode, lpSecurityAttributes,
  dwCreationDisposition, dwFlagsAndAttributes: LongWord; hTemplateFile: THandle): THandle;
  external 'CreateFileW@kernel32.dll stdcall';
function CloseHandle(hObject: THandle): Boolean;
  external 'CloseHandle@kernel32.dll stdcall';

function DefaultTerminalSupported(): Boolean;
var
  Version: TWindowsVersion;
  Revision: Cardinal;
begin
  GetWindowsVersionEx(Version);
  Revision := 0;
  RegQueryDWordValue(HKLM, 'SOFTWARE\Microsoft\Windows NT\CurrentVersion', 'UBR', Revision);
  Result := (Version.Major = 10) and ((Version.Build >= 22000) or
    ((Version.Build = 19045) and (Revision >= 3031)));
end;

function IntegrationInstalled(): Boolean;
begin
  Result := FileExists(ExpandConstant('{app}\windows-integration.enabled')) or
    FileExists(ExpandConstant('{app}\explorer-integration.enabled')) or
    FileExists(ExpandConstant('{app}\terminal-integration.enabled')) or
    (FileExists(ExpandConstant('{app}\windows-shell-integration.ps1')) and
     not FileExists(ExpandConstant('{app}\explorer-context-menu.ps1')));
end;

function ShouldSetDefaultTerminal(): Boolean;
begin
  { Silent updates retain integration without taking over a later user choice. }
  Result := WizardIsTaskSelected('defaultterminal') and
    (not WizardSilent or not IntegrationUpgrade or
     (Pos(',defaultterminal,', ',' + Lowercase(ExpandConstant('{param:TASKS|}')) + ',') > 0) or
     (Pos(',defaultterminal,', ',' + Lowercase(ExpandConstant('{param:MERGETASKS|}')) + ',') > 0));
end;

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
  if Action = 'Install' then
  begin
    if not WizardIsTaskSelected('explorer') then
      Parameters := Parameters + ' -DisableExplorerMenu';
    if ShouldSetDefaultTerminal() and not IsAdminInstallMode then
      Parameters := Parameters + ' -SetDefaultTerminal';
  end;
  Result := Exec(ExpandConstant('{sys}\WindowsPowerShell\v1.0\powershell.exe'),
    Parameters, '', SW_HIDE, ewWaitUntilTerminated, ExitCode);
  Log('Windows integration ' + Action + ': launched=' + IntToStr(Ord(Result)) + ', exit=' + IntToStr(ExitCode));
  Result := Result and (ExitCode = 0);
end;

function SetOriginalUserDefaultTerminal(): Boolean;
var
  ExitCode: Integer;
  Parameters, Directory: String;
begin
  Directory := ExpandConstant('{app}\shell-integration');
  { Register the chosen package for the original user before changing that user's default. }
  Parameters := '-NoLogo -NoProfile -NonInteractive -ExecutionPolicy Bypass -File ' +
    AddQuotes(Directory + '\windows-shell-integration.ps1') +
    ' -Action Install -Scope User' +
    ' -AppDirectory ' + AddQuotes(Directory) + ' -SetDefaultTerminal';
  if not WizardIsTaskSelected('explorer') then Parameters := Parameters + ' -DisableExplorerMenu';
  Result := ExecAsOriginalUser(ExpandConstant('{sys}\WindowsPowerShell\v1.0\powershell.exe'),
    Parameters, '', SW_HIDE, ewWaitUntilTerminated, ExitCode);
  Log('Default terminal for original user: launched=' + IntToStr(Ord(Result)) + ', exit=' + IntToStr(ExitCode));
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
  if WizardIsTaskSelected('defaultterminal') and not DefaultTerminalSupported() then
  begin
    Result := CustomMessage('UnsupportedTerminal');
    Exit;
  end;
  IntegrationUpgrade := IntegrationInstalled();
  if not ExecutableAvailable() then
  begin
    Result := CustomMessage('CloseBeforeInstall');
    Exit;
  end;
  if IntegrationInstalled() then
    { Unchecking the task only skips setting a default; upgrades preserve the current choice. }
    if not RunShellIntegration('Uninstall', True) then
      Result := CustomMessage('RemoveOldIntegrationFailed');
  if Result = '' then
  begin
    DeleteFile(ExpandConstant('{app}\explorer-integration.enabled'));
    DeleteFile(ExpandConstant('{app}\terminal-integration.enabled'));
    DeleteFile(ExpandConstant('{app}\windows-integration.enabled'));
  end;
end;

procedure CurStepChanged(CurStep: TSetupStep);
begin
  if CurStep = ssPostInstall then
  begin
    { Leave markers on partial failure so uninstall can still clean up registration. }
    if WizardIsTaskSelected('explorer') then
      SaveStringToFile(ExpandConstant('{app}\explorer-integration.enabled'), 'enabled', False);
    SaveStringToFile(ExpandConstant('{app}\windows-integration.enabled'), 'enabled', False);
    if not RunShellIntegration('Install', False) then
    begin
      IntegrationFailed := True;
      SuppressibleMsgBox(CustomMessage('IntegrationFailed') + #13#10 +
        ExpandConstant('{localappdata}\Edolterm\Logs\explorer-Install.log'), mbError, MB_OK, IDOK);
    end;
    if not IntegrationFailed and IsAdminInstallMode and ShouldSetDefaultTerminal() then
      if not SetOriginalUserDefaultTerminal() then
      begin
        IntegrationFailed := True;
        SuppressibleMsgBox(CustomMessage('DefaultTerminalFailed'), mbError, MB_OK, IDOK);
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
    MsgBox(CustomMessage('CloseBeforeUninstall'), mbError, MB_OK);
    Exit;
  end;
  if IntegrationInstalled() then
    if not RunShellIntegration('Uninstall', False) then
    begin
      MsgBox(CustomMessage('RemoveIntegrationFailed'), mbError, MB_OK);
      Result := False;
    end;
end;
