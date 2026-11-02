; Globw installer (Inno Setup).
; A guided wizard that installs gw.exe + gwget.exe, optionally adds Globw to the
; PATH (the user chooses), creates Start-menu entries and a clean uninstaller.
;
; Build it with Inno Setup 6:   iscc /DAppVersion=0.1.2 installer\globw.iss
; (the two binaries must already be built: cargo build --release --bins)

#ifndef AppVersion
  #define AppVersion "0.0.0"
#endif
#define AppName "Globw"
#define AppPublisher "Globw"
#define AppURL "https://globw.org"
#define AppExeName "gw.exe"

[Setup]
AppId={{B7A3F1C2-9E4D-4A6B-8C2F-1D5E7A9B3C4D}
AppName={#AppName}
AppVersion={#AppVersion}
AppVerName={#AppName} {#AppVersion}
AppPublisher={#AppPublisher}
AppPublisherURL={#AppURL}
AppSupportURL={#AppURL}
AppUpdatesURL={#AppURL}
DefaultDirName={autopf}\{#AppName}
DefaultGroupName={#AppName}
DisableProgramGroupPage=yes
LicenseFile=..\LICENSE
InfoBeforeFile=welcome.txt
OutputDir=output
OutputBaseFilename=globw-setup
SetupIconFile=globw.ico
UninstallDisplayIcon={app}\{#AppExeName}
UninstallDisplayName={#AppName}
WizardStyle=modern
Compression=lzma2/max
SolidCompression=yes
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
PrivilegesRequired=lowest
PrivilegesRequiredOverridesAllowed=dialog
ChangesEnvironment=yes

[Languages]
Name: "en"; MessagesFile: "compiler:Default.isl"
Name: "it"; MessagesFile: "compiler:Languages\Italian.isl"

[CustomMessages]
en.PathGroup=Environment:
it.PathGroup=Ambiente:
en.AddToPath=Add Globw to the PATH (run "gw" from any terminal)
it.AddToPath=Aggiungi Globw al PATH (usa "gw" da qualsiasi terminale)
en.OpenDocs=Open the Globw documentation
it.OpenDocs=Apri la documentazione di Globw
en.OpenExamples=Open the examples folder
it.OpenExamples=Apri la cartella degli esempi

[Tasks]
Name: "addtopath"; Description: "{cm:AddToPath}"; GroupDescription: "{cm:PathGroup}"

[Files]
Source: "..\target\release\gw.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\target\release\gwget.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\README.md"; DestDir: "{app}"; Flags: ignoreversion isreadme
Source: "..\LICENSE"; DestDir: "{app}"; Flags: ignoreversion
Source: "hello.gw"; DestDir: "{app}\examples"; Flags: ignoreversion

[Icons]
Name: "{group}\Globw Documentation"; Filename: "{#AppURL}/docs.html"
Name: "{group}\Globw Website"; Filename: "{#AppURL}"
Name: "{group}\Examples"; Filename: "{app}\examples"
Name: "{group}\{cm:UninstallProgram,{#AppName}}"; Filename: "{uninstallexe}"

[Run]
Filename: "{app}\examples"; Description: "{cm:OpenExamples}"; Flags: postinstall shellexec skipifsilent unchecked
Filename: "{#AppURL}/docs.html"; Description: "{cm:OpenDocs}"; Flags: postinstall shellexec skipifsilent nowait unchecked

[Code]
const
  EnvKeyUser = 'Environment';
  EnvKeyAll  = 'SYSTEM\CurrentControlSet\Control\Session Manager\Environment';

function EnvRoot: Integer;
begin
  if IsAdminInstallMode then Result := HKEY_LOCAL_MACHINE
  else Result := HKEY_CURRENT_USER;
end;

function EnvSubKey: string;
begin
  if IsAdminInstallMode then Result := EnvKeyAll
  else Result := EnvKeyUser;
end;

function PathHasDir(const Paths, Dir: string): Boolean;
var
  Tmp, Item: string;
  P: Integer;
begin
  Result := False;
  Tmp := Paths + ';';
  repeat
    P := Pos(';', Tmp);
    Item := Trim(Copy(Tmp, 1, P - 1));
    Delete(Tmp, 1, P);
    if CompareText(Item, Dir) = 0 then
    begin
      Result := True;
      Exit;
    end;
  until Tmp = '';
end;

procedure EnvAddPath(const Dir: string);
var
  Paths: string;
begin
  if not RegQueryStringValue(EnvRoot, EnvSubKey, 'Path', Paths) then
    Paths := '';
  if PathHasDir(Paths, Dir) then
    Exit;
  if (Paths <> '') and (Paths[Length(Paths)] <> ';') then
    Paths := Paths + ';';
  RegWriteExpandStringValue(EnvRoot, EnvSubKey, 'Path', Paths + Dir);
end;

procedure EnvRemovePath(const Dir: string);
var
  Paths, Rebuilt, Item: string;
  P: Integer;
begin
  if not RegQueryStringValue(EnvRoot, EnvSubKey, 'Path', Paths) then
    Exit;
  Rebuilt := '';
  Paths := Paths + ';';
  repeat
    P := Pos(';', Paths);
    Item := Trim(Copy(Paths, 1, P - 1));
    Delete(Paths, 1, P);
    if (Item <> '') and (CompareText(Item, Dir) <> 0) then
    begin
      if Rebuilt <> '' then
        Rebuilt := Rebuilt + ';';
      Rebuilt := Rebuilt + Item;
    end;
  until Paths = '';
  RegWriteExpandStringValue(EnvRoot, EnvSubKey, 'Path', Rebuilt);
end;

procedure CurStepChanged(CurStep: TSetupStep);
begin
  if CurStep = ssPostInstall then
    if WizardIsTaskSelected('addtopath') then
      EnvAddPath(ExpandConstant('{app}'));
end;

procedure CurUninstallStepChanged(CurUninstallStep: TUninstallStep);
begin
  if CurUninstallStep = usPostUninstall then
    EnvRemovePath(ExpandConstant('{app}'));
end;
