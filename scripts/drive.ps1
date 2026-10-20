param([string]$Exe, [string]$Arguments = '', [string]$Steps, [string]$OutDir, [int]$Width = 1440, [int]$Height = 860)
# Launch the app and run steps against its window with posted messages (the real mouse is not moved).
# Steps, separated by ';':  wait <ms> | click <x> <y> | dbl <x> <y> | right <x> <y> | move <x> <y> | key <vk> [ctrl] | text <chars> | shot <name>
#   | wheel <x> <y> <delta> [count] | cpu <label> (CPU time and wall time since the last cpu step) | ping <label> (time until the UI thread answers)
# Coordinates are client coordinates of the main window.
Add-Type -AssemblyName System.Drawing
Add-Type @"
using System; using System.Runtime.InteropServices;
public static class D {
  [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h, out RECT r);
  [DllImport("user32.dll")] public static extern bool MoveWindow(IntPtr h, int x, int y, int w, int hh, bool r);
  [DllImport("user32.dll")] public static extern bool PostMessage(IntPtr h, uint m, IntPtr w, IntPtr l);
  [DllImport("user32.dll")] public static extern bool PrintWindow(IntPtr h, IntPtr dc, uint f);
  [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr h);
  [DllImport("user32.dll")] public static extern bool ClientToScreen(IntPtr h, ref POINT p);
  [DllImport("user32.dll")] public static extern IntPtr SendMessageTimeout(IntPtr h, uint m, IntPtr w, IntPtr l, uint f, uint t, out IntPtr r);
  [StructLayout(LayoutKind.Sequential)] public struct RECT { public int L, T, R, B; }
  [StructLayout(LayoutKind.Sequential)] public struct POINT { public int X, Y; }
}
"@
$p = if ($Arguments) { Start-Process $Exe -ArgumentList $Arguments -PassThru } else { Start-Process $Exe -PassThru }
$sw = [Diagnostics.Stopwatch]::StartNew()
while ($p.MainWindowHandle -eq 0 -and $sw.ElapsedMilliseconds -lt 15000) { Start-Sleep -Milliseconds 50; $p.Refresh() }
$h = $p.MainWindowHandle
[void][D]::MoveWindow($h, 40, 40, $Width, $Height, $true)
[void][D]::SetForegroundWindow($h)
function MakeLParam($x, $y) { [IntPtr](([int]$y -shl 16) -bor ([int]$x -band 0xffff)) }
function Post($m, $w, $l) { [void][D]::PostMessage($h, $m, [IntPtr]$w, $l) }
$mark = @{ Cpu = $p.TotalProcessorTime; Wall = $sw.ElapsedMilliseconds }
foreach ($step in $Steps.Split(';')) {
  $a = $step.Trim().Split(' ')
  switch ($a[0]) {
    'wait' { Start-Sleep -Milliseconds ([int]$a[1]) }
    'move' { Post 0x200 0 (MakeLParam $a[1] $a[2]); Start-Sleep -Milliseconds 120 }
    'click' { $l = MakeLParam $a[1] $a[2]; Post 0x200 0 $l; Start-Sleep -Milliseconds 60; Post 0x201 1 $l; Start-Sleep -Milliseconds 40; Post 0x202 0 $l; Start-Sleep -Milliseconds 300 }
    'dbl' { $l = MakeLParam $a[1] $a[2]; Post 0x200 0 $l; Post 0x201 1 $l; Post 0x202 0 $l; Post 0x201 1 $l; Post 0x202 0 $l; Start-Sleep -Milliseconds 300 }
    'right' { $l = MakeLParam $a[1] $a[2]; Post 0x200 0 $l; Start-Sleep -Milliseconds 60; Post 0x204 2 $l; Start-Sleep -Milliseconds 40; Post 0x205 0 $l; Start-Sleep -Milliseconds 400 }
    'key' {
      $vk = [Convert]::ToInt32($a[1], 16)
      if ($a.Length -gt 2) { Post 0x100 0x11 0 }
      Post 0x100 $vk 0; Post 0x101 $vk 0
      if ($a.Length -gt 2) { Post 0x101 0x11 0 }
      Start-Sleep -Milliseconds 300
    }
    'text' { foreach ($c in $a[1].ToCharArray()) { Post 0x102 ([int]$c) 0 }; Start-Sleep -Milliseconds 300 }
    'wheel' {
      # WM_MOUSEWHEEL takes screen coordinates; the delta is the high word of wParam (120 = one notch, negative scrolls down)
      $pt = New-Object D+POINT; $pt.X = [int]$a[1]; $pt.Y = [int]$a[2]; [void][D]::ClientToScreen($h, [ref]$pt)
      $n = if ($a.Length -gt 4) { [int]$a[4] } else { 1 }
      Post 0x200 0 (MakeLParam $a[1] $a[2])
      for ($i = 0; $i -lt $n; $i++) { Post 0x20A ([int]$a[3] -shl 16) (MakeLParam $pt.X $pt.Y); Start-Sleep -Milliseconds 16 }
    }
    'cpu' {
      $p.Refresh()
      "{0}: cpu {1:N0} ms, wall {2} ms" -f $a[1], ($p.TotalProcessorTime - $mark.Cpu).TotalMilliseconds, ($sw.ElapsedMilliseconds - $mark.Wall)
      $mark = @{ Cpu = $p.TotalProcessorTime; Wall = $sw.ElapsedMilliseconds }
    }
    'ping' { $t0 = $sw.ElapsedMilliseconds; $r = [IntPtr]::Zero; [void][D]::SendMessageTimeout($h, 0, [IntPtr]::Zero, [IntPtr]::Zero, 2, 20000, [ref]$r); "{0}: ui answered after {1} ms" -f $a[1], ($sw.ElapsedMilliseconds - $t0) }
    'shot' {
      $r = New-Object D+RECT
      [void][D]::GetWindowRect($h, [ref]$r)
      $bmp = New-Object Drawing.Bitmap ($r.R - $r.L), ($r.B - $r.T)
      $g = [Drawing.Graphics]::FromImage($bmp); $dc = $g.GetHdc()
      [void][D]::PrintWindow($h, $dc, 2); $g.ReleaseHdc($dc)
      $bmp.Save((Join-Path $OutDir "$($a[1]).png"))
    }
  }
}
Stop-Process -Id $p.Id -Force
