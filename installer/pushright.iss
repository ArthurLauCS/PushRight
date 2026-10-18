; PushRight 安装包（Inno Setup 6）。用 scripts\installer.ps1 构建：它先下载字体、编译，再传入版本号。
; 按当前用户安装到 %LOCALAPPDATA%\Programs\PushRight，不需要管理员权限。
; 装过旧版（Tauri 打的 NSIS 安装包，在 %LOCALAPPDATA%\PushRight）时先把它静默卸载掉，开始菜单里不会出现两个 PushRight。

#ifndef Version
  #define Version "0.0.0"
#endif

[Setup]
AppId=PushRight
AppName=PushRight
AppVersion={#Version}
AppVerName=PushRight {#Version}
AppPublisher=ArthurLau
AppPublisherURL=https://github.com/ArthurLauCS/PushRight
DefaultDirName={autopf}\PushRight
DisableProgramGroupPage=yes
PrivilegesRequired=lowest
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
MinVersion=10.0
OutputDir=..\target\installer
OutputBaseFilename=PushRight-Setup-{#Version}
SetupIconFile=..\crates\app\assets\app.ico
UninstallDisplayIcon={app}\pushright.exe
Compression=lzma2/max
SolidCompression=yes
WizardStyle=modern
UsePreviousLanguage=no
; 升级时结束正在运行的实例
CloseApplications=force
RestartApplications=no

[Languages]
Name: "zh"; MessagesFile: "ChineseSimplified.isl"
Name: "en"; MessagesFile: "compiler:Default.isl"

[CustomMessages]
zh.RemoveOld=正在移除旧版 PushRight…
en.RemoveOld=Removing the previous PushRight...

[Tasks]
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; Flags: unchecked

[Files]
Source: "..\target\release\pushright.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\crates\app\assets\fonts\*.ttf"; DestDir: "{app}\fonts"; Flags: ignoreversion
Source: "..\LICENSE"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\THIRD_PARTY_NOTICES.txt"; DestDir: "{app}"; Flags: ignoreversion

[Icons]
Name: "{autoprograms}\PushRight"; Filename: "{app}\pushright.exe"
Name: "{autodesktop}\PushRight"; Filename: "{app}\pushright.exe"; Tasks: desktopicon

[Run]
Filename: "{app}\pushright.exe"; Description: "{cm:LaunchProgram,PushRight}"; Flags: nowait postinstall skipifsilent

[UninstallDelete]
; 界面偏好（页签、最近打开、布局）一起删
Type: filesandordirs; Name: "{userappdata}\PushRight"

[Code]
const
  OldKey = 'Software\Microsoft\Windows\CurrentVersion\Uninstall\PushRight';

// 旧版是 NSIS 装的：卸载程序加 /S 静默卸载，不删它的用户数据
procedure CurStepChanged(CurStep: TSetupStep);
var
  Uninstaller: String;
  Code: Integer;
begin
  if (CurStep = ssInstall) and (ExpandConstant('{param:KEEPOLD|0}') = '0') and RegQueryStringValue(HKCU, OldKey, 'UninstallString', Uninstaller) then
  begin
    WizardForm.StatusLabel.Caption := CustomMessage('RemoveOld');
    Exec(RemoveQuotes(Uninstaller), '/S', '', SW_HIDE, ewWaitUntilTerminated, Code);
  end;
end;
