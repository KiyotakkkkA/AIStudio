param([int]$ProcessId)
Add-Type @"
using System;
using System.Runtime.InteropServices;
using System.Text;
public static class ZvsWindows {
  public delegate bool Callback(IntPtr hwnd, IntPtr data);
  [DllImport("user32.dll")] static extern bool EnumWindows(Callback callback, IntPtr data);
  [DllImport("user32.dll")] static extern bool EnumChildWindows(IntPtr parent, Callback callback, IntPtr data);
  [DllImport("user32.dll")] static extern uint GetWindowThreadProcessId(IntPtr hwnd, out uint pid);
  [DllImport("user32.dll")] static extern bool IsWindowVisible(IntPtr hwnd);
  [DllImport("user32.dll")] static extern IntPtr GetParent(IntPtr hwnd);
  [DllImport("user32.dll", CharSet = CharSet.Unicode)] static extern int GetClassName(IntPtr hwnd, StringBuilder name, int size);
  static string Class(IntPtr hwnd) { var name = new StringBuilder(256); GetClassName(hwnd, name, 256); return name.ToString(); }
  public static string Count(uint target) {
    int windows = 0, webviews = 0;
    EnumWindows((top, _) => {
      uint pid; GetWindowThreadProcessId(top, out pid);
      if (pid != target || Class(top) != "Tauri Window" || !IsWindowVisible(top)) return true;
      windows++;
      EnumChildWindows(top, (child, __) => {
        if (GetParent(child) == top && Class(child) == "WRY_WEBVIEW" && IsWindowVisible(child)) webviews++;
        return true;
      }, IntPtr.Zero);
      return true;
    }, IntPtr.Zero);
    return "{\"windows\":" + windows + ",\"webviews\":" + webviews + "}";
  }
}
"@
[ZvsWindows]::Count([uint32]$ProcessId)
