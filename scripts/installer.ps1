# 下载字体、编译 release、打安装包，产物在 target\installer\PushRight-Setup-<版本>.exe。需要 Inno Setup 6。
$root = "$PSScriptRoot\.."
$out = "$root\target\installer"
# 只留本次版本的产物，旧版本的安装包不累计
Remove-Item -Recurse -Force $out -ErrorAction SilentlyContinue
& "$PSScriptRoot\fonts.ps1"
cargo build --release -p pushright --manifest-path "$root\Cargo.toml"
if ($LASTEXITCODE) { exit $LASTEXITCODE }
$version = (Select-String -Path "$root\crates\app\Cargo.toml" -Pattern '^version = "(.+)"' | Select-Object -First 1).Matches[0].Groups[1].Value
# Inno Setup 装在 Program Files 或按用户装在 LocalAppData 下
$iscc = @("${env:ProgramFiles(x86)}\Inno Setup 6\ISCC.exe", "$env:LOCALAPPDATA\Programs\Inno Setup 6\ISCC.exe") | Where-Object { Test-Path $_ } | Select-Object -First 1
& $iscc /Q "/DVersion=$version" "$root\installer\pushright.iss"
exit $LASTEXITCODE
