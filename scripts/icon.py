"""由 crates/app/assets/icon.svg（40px 及以上）和 icon-small.svg（16～32px）生成应用图标。

产物都在 crates/app/assets：app.ico、app.res（build.rs 链接进 exe）、icon-128.png（VS Code 扩展的图标）。
用无头 Chrome 按每个尺寸单独栅格化（小图标不是大图缩小，线条才清楚），Pillow 裁切，rc.exe 编资源。
改了 SVG 后运行：python scripts/icon.py
"""
import glob
import io
import struct
import subprocess
import tempfile
from pathlib import Path

from PIL import Image

ROOT = Path(__file__).resolve().parent.parent
ASSETS = ROOT / "crates" / "app" / "assets"
CHROME = r"C:\Program Files\Google\Chrome\Application\chrome.exe"
SIZES = [16, 20, 24, 32, 40, 48, 64, 128, 256]


def render() -> dict[int, Image.Image]:
    # 所有尺寸横排在一页里截一次图，再按位置裁出来
    x, cells = 0, []
    for s in SIZES:
        svg = (ASSETS / ("icon-small.svg" if s <= 32 else "icon.svg")).as_uri()
        cells.append(f'<img src="{svg}" style="position:absolute;left:{x}px;top:0;width:{s}px;height:{s}px">')
        x += s + 8
    with tempfile.TemporaryDirectory() as tmp:
        page, shot = Path(tmp) / "sheet.html", Path(tmp) / "sheet.png"
        page.write_text("<body style='margin:0'>" + "".join(cells) + "</body>", encoding="utf-8")
        subprocess.run([CHROME, "--headless=new", "--hide-scrollbars", "--force-device-scale-factor=1",
                        "--default-background-color=00000000", f"--window-size={x},{max(SIZES)}",
                        f"--screenshot={shot}", page.as_uri()], check=True, capture_output=True)
        sheet = Image.open(shot).convert("RGBA")
    out, x = {}, 0
    for s in SIZES:
        out[s] = sheet.crop((x, 0, x + s, s))
        x += s + 8
    return out


def write_ico(images: dict[int, Image.Image], path: Path):
    # 每个尺寸都存 PNG 数据，Vista 以后的系统都认
    blobs = []
    for s, im in images.items():
        b = io.BytesIO()
        im.save(b, "PNG")
        blobs.append((s, b.getvalue()))
    head = struct.pack("<HHH", 0, 1, len(blobs))
    offset, dirs = 6 + 16 * len(blobs), b""
    for s, data in blobs:
        dirs += struct.pack("<BBBBHHII", s % 256, s % 256, 0, 0, 1, 32, len(data), offset)
        offset += len(data)
    path.write_bytes(head + dirs + b"".join(d for _, d in blobs))


def main():
    images = render()
    write_ico(images, ASSETS / "app.ico")
    images[128].save(ASSETS / "icon-128.png")
    # 预览：浅色、深色底各一行，检查边缘和小尺寸的清晰度
    w = sum(s + 16 for s in SIZES)
    prev = Image.new("RGBA", (w, 2 * (max(SIZES) + 16)), "#F4F2EA")
    prev.paste("#1F1E1D", (0, max(SIZES) + 16, w, prev.height))
    x = 8
    for s in SIZES:
        for row in range(2):
            prev.alpha_composite(images[s], (x, 8 + row * (max(SIZES) + 16)))
        x += s + 16
    (ROOT / "target").mkdir(exist_ok=True)
    prev.save(ROOT / "target" / "icon-preview.png")
    # 资源号 1：资源管理器和 gpui 的窗口图标都取 exe 里编号为 1 的图标组
    rc = sorted(glob.glob(r"C:\Program Files (x86)\Windows Kits\10\bin\*\x64\rc.exe"))[-1]
    (ASSETS / "app.rc").write_text('1 ICON "app.ico"\n', encoding="ascii")
    subprocess.run([rc, "/nologo", "/fo", "app.res", "app.rc"], cwd=ASSETS, check=True)
    (ASSETS / "app.rc").unlink()


if __name__ == "__main__":
    main()
