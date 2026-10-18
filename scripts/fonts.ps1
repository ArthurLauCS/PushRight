# 下载界面用的两套开源中文字体到 crates/app/assets/fonts（不进 git，约 50 MB）。
# 开发运行和打安装包前各跑一次；文件已在且哈希对得上就跳过。字体许可证都是 SIL OFL 1.1。
$dir = "$PSScriptRoot\..\crates\app\assets\fonts"
New-Item -ItemType Directory -Force $dir | Out-Null
$fonts = @(
    # 标题和名字：思源宋体（可变字重）
    @{ Name = 'NotoSerifSC-VF.ttf'; Url = 'https://github.com/notofonts/noto-cjk/raw/Serif2.003/Serif/Variable/TTF/Subset/NotoSerifSC-VF.ttf'; Sha256 = '5326cfb097e3ab26fcb39329752b5c0a439bf8d5c4649520e4b492939c352a09' },
    # 给人读的整段话：霞鹜文楷
    @{ Name = 'LXGWWenKai-Regular.ttf'; Url = 'https://github.com/lxgw/LxgwWenKai/releases/download/v1.530/LXGWWenKai-Regular.ttf'; Sha256 = '2d73eea94ad808999d90c0000c239a84a33b09d4bc6ab6c5b8bbf1eda8a7fc8b' }
)
foreach ($f in $fonts) {
    $path = Join-Path $dir $f.Name
    if ((Test-Path $path) -and (Get-FileHash $path -Algorithm SHA256).Hash -eq $f.Sha256) { continue }
    Invoke-WebRequest $f.Url -OutFile $path -UseBasicParsing
    $hash = (Get-FileHash $path -Algorithm SHA256).Hash
    if ($hash -ne $f.Sha256) { throw "$($f.Name) SHA-256 mismatch: $hash" }
}
