# 下载界面用的三套开源字体（都带中文）到 crates/app/assets/fonts（不进 git，约 70 MB）。
# 开发运行和打安装包前各跑一次；文件已在且哈希对得上就跳过。字体许可证都是 SIL OFL 1.1。
$dir = "$PSScriptRoot\..\crates\app\assets\fonts"
New-Item -ItemType Directory -Force $dir | Out-Null
$fonts = @(
    # 标题和名字：思源宋体（可变字重）
    @{ Name = 'NotoSerifSC-VF.ttf'; Url = 'https://github.com/notofonts/noto-cjk/raw/Serif2.003/Serif/Variable/TTF/Subset/NotoSerifSC-VF.ttf'; Sha256 = '5326cfb097e3ab26fcb39329752b5c0a439bf8d5c4649520e4b492939c352a09' },
    # 给人读的整段话：霞鹜文楷
    @{ Name = 'LXGWWenKai-Regular.ttf'; Url = 'https://github.com/lxgw/LxgwWenKai/releases/download/v1.530/LXGWWenKai-Regular.ttf'; Sha256 = '2d73eea94ad808999d90c0000c239a84a33b09d4bc6ab6c5b8bbf1eda8a7fc8b' },
    # 代码、哈希、路径：Maple Mono CN，中文正好两个英文字符宽。只随约 140 MB 的压缩包发布，从里面取出 Regular 一个文件
    @{ Name = 'MapleMono-CN-Regular.ttf'; Url = 'https://github.com/subframe7536/maple-font/releases/download/v7.9/MapleMono-CN.zip'; Sha256 = 'e6338c3dca9a3bed3fb81f43c8bb8feca3b6ad54829f5a3229527d030a321dbd' }
)
foreach ($f in $fonts) {
    $path = Join-Path $dir $f.Name
    if ((Test-Path $path) -and (Get-FileHash $path -Algorithm SHA256).Hash -eq $f.Sha256) { continue }
    if ($f.Url.EndsWith('.zip')) {
        $zip = "$path.zip"
        Invoke-WebRequest $f.Url -OutFile $zip -UseBasicParsing
        Add-Type -AssemblyName System.IO.Compression.FileSystem
        $archive = [IO.Compression.ZipFile]::OpenRead($zip)
        [IO.Compression.ZipFileExtensions]::ExtractToFile($archive.GetEntry($f.Name), $path, $true)
        $archive.Dispose()
        Remove-Item $zip
    } else {
        Invoke-WebRequest $f.Url -OutFile $path -UseBasicParsing
    }
    $hash = (Get-FileHash $path -Algorithm SHA256).Hash
    if ($hash -ne $f.Sha256) { throw "$($f.Name) SHA-256 mismatch: $hash" }
}
