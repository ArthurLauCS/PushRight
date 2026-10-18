# 下载字体、编译 release、打安装包，产物在 target\installer\PushRight-Setup-<版本>.exe。需要 Inno Setup 6。
$root = "$PSScriptRoot\.."
$out = "$root\target\installer"
# 只留本次版本的产物，旧版本的安装包不累计
Remove-Item -Recurse -Force $out -ErrorAction SilentlyContinue
& "$PSScriptRoot\fonts.ps1"
cargo build --release -p pushright --manifest-path "$root\Cargo.toml"
if ($LASTEXITCODE) { exit $LASTEXITCODE }
$version = (Select-String -Path "$root\crates\app\Cargo.toml" -Pattern '^version = "(.+)"' | Select-Object -First 1).Matches[0].Groups[1].Value
& "${env:ProgramFiles(x86)}\Inno Setup 6\ISCC.exe" /Q "/DVersion=$version" "$root\installer\pushright.iss"
exit $LASTEXITCODE
