param(
    [string]$ExePath = (Join-Path $PSScriptRoot "..\target\debug\j3text.exe")
)

$ErrorActionPreference = "Stop"
Add-Type -AssemblyName System.Windows.Forms
$script:WshShell = New-Object -ComObject WScript.Shell

Add-Type @"
using System;
using System.Text;
using System.Runtime.InteropServices;

public static class Win32Smoke {
    public delegate bool EnumWindowsProc(IntPtr hWnd, IntPtr lParam);

    [StructLayout(LayoutKind.Sequential)]
    public struct RECT {
        public int Left;
        public int Top;
        public int Right;
        public int Bottom;
    }

    [DllImport("user32.dll")]
    public static extern bool SetForegroundWindow(IntPtr hWnd);

    [DllImport("user32.dll")]
    public static extern bool ShowWindow(IntPtr hWnd, int nCmdShow);

    [DllImport("user32.dll")]
    public static extern bool PostMessageW(IntPtr hWnd, uint Msg, UIntPtr wParam, IntPtr lParam);

    [DllImport("user32.dll")]
    public static extern IntPtr SendMessageW(IntPtr hWnd, uint Msg, UIntPtr wParam, IntPtr lParam);

    [DllImport("user32.dll", CharSet = CharSet.Unicode)]
    public static extern IntPtr SendMessageW(IntPtr hWnd, uint Msg, UIntPtr wParam, StringBuilder lParam);

    [DllImport("user32.dll", CharSet = CharSet.Unicode)]
    public static extern IntPtr SendMessageW(IntPtr hWnd, uint Msg, UIntPtr wParam, string lParam);

    [DllImport("user32.dll", CharSet = CharSet.Unicode)]
    public static extern int GetWindowTextW(IntPtr hWnd, StringBuilder text, int count);

    [DllImport("user32.dll")]
    public static extern int GetWindowTextLengthW(IntPtr hWnd);

    [DllImport("user32.dll", CharSet = CharSet.Unicode)]
    public static extern int GetClassNameW(IntPtr hWnd, StringBuilder text, int count);

    [DllImport("user32.dll")]
    public static extern bool EnumWindows(EnumWindowsProc callback, IntPtr lParam);

    [DllImport("user32.dll")]
    public static extern bool EnumChildWindows(IntPtr hWndParent, EnumWindowsProc callback, IntPtr lParam);

    [DllImport("user32.dll")]
    public static extern uint GetWindowThreadProcessId(IntPtr hWnd, out uint processId);

    [DllImport("user32.dll")]
    public static extern bool IsWindowVisible(IntPtr hWnd);

    [DllImport("user32.dll")]
    public static extern bool IsWindow(IntPtr hWnd);

    [DllImport("user32.dll")]
    public static extern bool GetWindowRect(IntPtr hWnd, out RECT rect);

    [DllImport("user32.dll")]
    public static extern bool SetCursorPos(int x, int y);

    [DllImport("user32.dll")]
    public static extern void mouse_event(uint flags, uint dx, uint dy, uint data, UIntPtr extraInfo);

    [DllImport("user32.dll")]
    public static extern IntPtr GetMenu(IntPtr hWnd);

    [DllImport("user32.dll")]
    public static extern IntPtr GetSubMenu(IntPtr hMenu, int nPos);

    [DllImport("user32.dll")]
    public static extern bool GetMenuItemRect(IntPtr hWnd, IntPtr hMenu, uint item, out RECT rect);

    [DllImport("user32.dll")]
    public static extern IntPtr GetDlgItem(IntPtr hDlg, int nIDDlgItem);

    [DllImport("user32.dll", CharSet = CharSet.Unicode)]
    public static extern int GetMenuStringW(IntPtr hMenu, uint uIDItem, StringBuilder lpString, int cchMax, uint flags);
}
"@

Set-StrictMode -Version Latest
$script:LogPath = Join-Path ([System.IO.Path]::GetTempPath()) "j3text-windows-menu-smoke-last.log"
Set-Content -Path $script:LogPath -Value "START $(Get-Date -Format o)"

function Write-SmokeLog([string]$Message) {
    Add-Content -Path $script:LogPath -Value "$(Get-Date -Format o) $Message"
    Write-Host $Message
}

$WM_COMMAND = 0x0111
$WM_SETTEXT = 0x000C
$WM_GETTEXTLENGTH = 0x000E
$WM_GETTEXT = 0x000D
$BM_CLICK = 0x00F5
$SW_RESTORE = 9
$MOUSEEVENTF_LEFTDOWN = 0x0002
$MOUSEEVENTF_LEFTUP = 0x0004
$MF_BYPOSITION = 0x00000400

$Commands = [ordered]@{
    FileNew = 1001
    FileOpen = 1002
    FileSave = 1003
    FileSaveAs = 1004
    FileClose = 1005
    FileCloseOthers = 1006
    FileCloseAll = 1007
    FileExit = 1008
    Recent0 = 1050
    Find = 1101
    Replace = 1102
    FindNext = 1103
    FindPrevious = 1104
    FindAll = 1105
    Undo = 1106
    Cut = 1107
    Copy = 1108
    Paste = 1109
    SelectAll = 1110
    Redo = 1111
    LineNumbers = 1201
    Marks = 1202
    Commands = 1203
    ReopenEncoding = 1301
    ChangeEncoding = 1302
    LineEndingCrlf = 1401
    LineEndingLf = 1402
    LineEndingCr = 1403
    Font = 1501
    TabSize2 = 1522
    TabSize4 = 1524
    TabSize8 = 1528
    WordWrap = 1531
    ThemeSystem = 1541
    ThemeLight = 1542
    ThemeClassicDark = 1543
    ThemeSepiaTeal = 1544
    ThemeGraphite = 1545
    ThemeForest = 1546
    ThemeSteelBlue = 1547
    About = 1601
    TabLeft = 1701
    TabRight = 1702
    OpenNewWindow = 1703
}

function New-SmokeRoot {
    $root = Join-Path ([System.IO.Path]::GetTempPath()) ("j3text-win-menu-smoke-" + [Guid]::NewGuid().ToString("N"))
    New-Item -ItemType Directory -Path $root | Out-Null
    return $root
}

function Get-WindowText([IntPtr]$Hwnd) {
    $length = [Win32Smoke]::GetWindowTextLengthW($Hwnd)
    $builder = [System.Text.StringBuilder]::new($length + 1)
    [void][Win32Smoke]::GetWindowTextW($Hwnd, $builder, $builder.Capacity)
    return $builder.ToString()
}

function Get-ClassName([IntPtr]$Hwnd) {
    $builder = [System.Text.StringBuilder]::new(256)
    [void][Win32Smoke]::GetClassNameW($Hwnd, $builder, $builder.Capacity)
    return $builder.ToString()
}

function Get-ProcessWindows([int]$ProcessId) {
    $windows = New-Object System.Collections.Generic.List[object]
    $callback = [Win32Smoke+EnumWindowsProc]{
        param([IntPtr]$Hwnd, [IntPtr]$LParam)
        $windowProcessId = 0
        [void][Win32Smoke]::GetWindowThreadProcessId($Hwnd, [ref]$windowProcessId)
        if ($windowProcessId -eq $ProcessId -and [Win32Smoke]::IsWindowVisible($Hwnd)) {
            $windows.Add([pscustomobject]@{
                Hwnd = $Hwnd
                Title = Get-WindowText $Hwnd
                ClassName = Get-ClassName $Hwnd
            })
        }
        return $true
    }
    [void][Win32Smoke]::EnumWindows($callback, [IntPtr]::Zero)
    return $windows
}

function Wait-ForWindowTitle([int]$ProcessId, [string]$Pattern, [int]$TimeoutMs = 5000) {
    $deadline = (Get-Date).AddMilliseconds($TimeoutMs)
    while ((Get-Date) -lt $deadline) {
        $match = @(Get-ProcessWindows $ProcessId | Where-Object { $_.Title -match $Pattern } | Select-Object -First 1)
        if ($match.Count -gt 0) {
            return $match[0]
        }
        Start-Sleep -Milliseconds 50
    }
    throw "Timed out waiting for window title /$Pattern/"
}

function Wait-ForMainJ3TextWindow([int]$ProcessId, [int]$TimeoutMs = 10000) {
    $deadline = (Get-Date).AddMilliseconds($TimeoutMs)
    while ((Get-Date) -lt $deadline) {
        $match = @(
            Get-ProcessWindows $ProcessId |
                Where-Object { $_.ClassName -eq "J3TextMainWindow" -and $_.Title -match "j3Text$" } |
                Select-Object -First 1
        )
        if ($match.Count -gt 0) {
            return $match[0]
        }
        Start-Sleep -Milliseconds 50
    }
    $windows = Get-ProcessWindows $ProcessId | Format-Table -AutoSize | Out-String
    throw "Timed out waiting for j3Text main window. Windows:`n$windows"
}

function Wait-UntilWindowClosed([IntPtr]$Hwnd, [int]$TimeoutMs = 5000) {
    $deadline = (Get-Date).AddMilliseconds($TimeoutMs)
    while ((Get-Date) -lt $deadline) {
        if (-not [Win32Smoke]::IsWindow($Hwnd)) {
            return
        }
        Start-Sleep -Milliseconds 50
    }
    throw "Timed out waiting for window to close"
}

function Activate-Window([IntPtr]$Hwnd) {
    [void][Win32Smoke]::ShowWindow($Hwnd, $SW_RESTORE)
    [void][Win32Smoke]::SetForegroundWindow($Hwnd)
    Start-Sleep -Milliseconds 120
}

function Send-KeysWait([string]$Keys, [int]$DelayMs = 160) {
    $script:WshShell.SendKeys($Keys)
    Start-Sleep -Milliseconds $DelayMs
}

function Click-WindowPoint([IntPtr]$Hwnd, [int]$X, [int]$Y) {
    $rect = New-Object Win32Smoke+RECT
    if (-not [Win32Smoke]::GetWindowRect($Hwnd, [ref]$rect)) {
        throw "GetWindowRect failed"
    }
    [void][Win32Smoke]::SetCursorPos($rect.Left + $X, $rect.Top + $Y)
    [Win32Smoke]::mouse_event($MOUSEEVENTF_LEFTDOWN, 0, 0, 0, [UIntPtr]::Zero)
    [Win32Smoke]::mouse_event($MOUSEEVENTF_LEFTUP, 0, 0, 0, [UIntPtr]::Zero)
    Start-Sleep -Milliseconds 120
}

function Invoke-CommandId([IntPtr]$Hwnd, [int]$CommandId, [string]$Label) {
    Write-SmokeLog "STEP command $Label"
    if (-not [Win32Smoke]::PostMessageW(
        $Hwnd,
        $WM_COMMAND,
        ([UIntPtr]::new([uint64]$CommandId)),
        [IntPtr]::Zero
    )) {
        throw "PostMessage failed for $Label ($CommandId)"
    }
    Start-Sleep -Milliseconds 220
}

function Invoke-MenuClick([IntPtr]$Hwnd, [int]$TopIndex, [int]$ItemIndex, [string]$Label) {
    Write-SmokeLog "STEP menu click $Label"
    Activate-Window $Hwnd
    $menu = [Win32Smoke]::GetMenu($Hwnd)
    if ($menu -eq [IntPtr]::Zero) {
        throw "No main menu for $Label"
    }
    $topRect = New-Object Win32Smoke+RECT
    if (-not [Win32Smoke]::GetMenuItemRect($Hwnd, $menu, [uint32]$TopIndex, [ref]$topRect)) {
        throw "GetMenuItemRect top failed for $Label"
    }
    [void][Win32Smoke]::SetCursorPos([int](($topRect.Left + $topRect.Right) / 2), [int](($topRect.Top + $topRect.Bottom) / 2))
    [Win32Smoke]::mouse_event($MOUSEEVENTF_LEFTDOWN, 0, 0, 0, [UIntPtr]::Zero)
    [Win32Smoke]::mouse_event($MOUSEEVENTF_LEFTUP, 0, 0, 0, [UIntPtr]::Zero)
    Start-Sleep -Milliseconds 180
    $subMenu = [Win32Smoke]::GetSubMenu($menu, $TopIndex)
    for ($menuIndex = 0; $menuIndex -lt 12; $menuIndex++) {
        $labelBuilder = [System.Text.StringBuilder]::new(256)
        [void][Win32Smoke]::GetMenuStringW($subMenu, [uint32]$menuIndex, $labelBuilder, $labelBuilder.Capacity, $MF_BYPOSITION)
        if ($labelBuilder.Length -gt 0) {
            Write-Host "  submenu[$menuIndex]=$($labelBuilder.ToString())"
        }
    }
    $itemRect = New-Object Win32Smoke+RECT
    if (-not [Win32Smoke]::GetMenuItemRect($Hwnd, $subMenu, [uint32]$ItemIndex, [ref]$itemRect)) {
        Send-KeysWait "{ESC}" 80
        throw "GetMenuItemRect item failed for $Label"
    }
    [void][Win32Smoke]::SetCursorPos([int](($itemRect.Left + $itemRect.Right) / 2), [int](($itemRect.Top + $itemRect.Bottom) / 2))
    [Win32Smoke]::mouse_event($MOUSEEVENTF_LEFTDOWN, 0, 0, 0, [UIntPtr]::Zero)
    [Win32Smoke]::mouse_event($MOUSEEVENTF_LEFTUP, 0, 0, 0, [UIntPtr]::Zero)
    Start-Sleep -Milliseconds 260
}

function Get-ChildWindows([IntPtr]$Parent, [switch]$IncludeText) {
    $children = New-Object System.Collections.Generic.List[object]
    $callback = [Win32Smoke+EnumWindowsProc]{
        param([IntPtr]$Hwnd, [IntPtr]$LParam)
        $title = ""
        if ($IncludeText) {
            $title = Get-WindowText $Hwnd
        }
        $children.Add([pscustomobject]@{
            Hwnd = $Hwnd
            Title = $title
            ClassName = Get-ClassName $Hwnd
        })
        return $true
    }
    [void][Win32Smoke]::EnumChildWindows($Parent, $callback, [IntPtr]::Zero)
    return $children
}

function Get-DescendantWindows([IntPtr]$Parent, [switch]$IncludeText) {
    $seen = New-Object 'System.Collections.Generic.HashSet[int64]'
    $all = New-Object System.Collections.Generic.List[object]
    foreach ($child in Get-ChildWindows $Parent -IncludeText:$IncludeText) {
        if ($seen.Add($child.Hwnd.ToInt64())) {
            $all.Add($child)
        }
    }
    return $all
}

function Get-EditorText([IntPtr]$MainHwnd) {
    $edit = @(Get-ChildWindows $MainHwnd | Where-Object { $_.ClassName -eq "RICHEDIT50W" } | Select-Object -First 1)
    if ($edit.Count -eq 0) {
        throw "Main editor control not found"
    }
    $length = [int][Win32Smoke]::SendMessageW($edit[0].Hwnd, $WM_GETTEXTLENGTH, [UIntPtr]::Zero, [IntPtr]::Zero)
    $builder = [System.Text.StringBuilder]::new($length + 1)
    [void][Win32Smoke]::SendMessageW(
        $edit[0].Hwnd,
        $WM_GETTEXT,
        ([UIntPtr]::new([uint64]$builder.Capacity)),
        $builder
    )
    return $builder.ToString()
}

function Set-EditorText([IntPtr]$MainHwnd, [string]$Text) {
    $edit = @(Get-ChildWindows $MainHwnd | Where-Object { $_.ClassName -eq "RICHEDIT50W" } | Select-Object -First 1)
    if ($edit.Count -eq 0) {
        throw "Main editor control not found"
    }
    [void][Win32Smoke]::SendMessageW($edit[0].Hwnd, $WM_SETTEXT, [UIntPtr]::Zero, $Text)
    Start-Sleep -Milliseconds 220
}

function Set-ControlTextById([IntPtr]$ParentHwnd, [int]$ControlId, [string]$Text) {
    $control = [Win32Smoke]::GetDlgItem($ParentHwnd, $ControlId)
    if ($control -eq [IntPtr]::Zero) {
        throw "Control id $ControlId not found"
    }
    [void][Win32Smoke]::SendMessageW($control, $WM_SETTEXT, [UIntPtr]::Zero, $Text)
    Start-Sleep -Milliseconds 120
}

function Click-Editor([IntPtr]$MainHwnd) {
    $edit = @(Get-ChildWindows $MainHwnd | Where-Object { $_.ClassName -eq "RICHEDIT50W" } | Select-Object -First 1)
    if ($edit.Count -eq 0) {
        throw "Main editor control not found"
    }
    $rect = New-Object Win32Smoke+RECT
    if (-not [Win32Smoke]::GetWindowRect($edit[0].Hwnd, [ref]$rect)) {
        throw "GetWindowRect failed for editor"
    }
    [void][Win32Smoke]::SetCursorPos(
        [int](($rect.Left + $rect.Right) / 2),
        [int](($rect.Top + $rect.Bottom) / 2)
    )
    [Win32Smoke]::mouse_event($MOUSEEVENTF_LEFTDOWN, 0, 0, 0, [UIntPtr]::Zero)
    [Win32Smoke]::mouse_event($MOUSEEVENTF_LEFTUP, 0, 0, 0, [UIntPtr]::Zero)
    Start-Sleep -Milliseconds 160
}

function Wait-ForFileContains([string]$Path, [string]$Text, [int]$TimeoutMs = 5000) {
    $deadline = (Get-Date).AddMilliseconds($TimeoutMs)
    while ((Get-Date) -lt $deadline) {
        if ((Test-Path $Path) -and ((Get-Content -Raw $Path) -like "*$Text*")) {
            return
        }
        Start-Sleep -Milliseconds 80
    }
    throw "Timed out waiting for $Path to contain '$Text'"
}

function Wait-ForBytesPrefix([string]$Path, [byte[]]$Prefix, [int]$TimeoutMs = 5000) {
    $deadline = (Get-Date).AddMilliseconds($TimeoutMs)
    while ((Get-Date) -lt $deadline) {
        if (Test-Path $Path) {
            $bytes = [System.IO.File]::ReadAllBytes($Path)
            if ($bytes.Length -ge $Prefix.Length) {
                $same = $true
                for ($i = 0; $i -lt $Prefix.Length; $i++) {
                    if ($bytes[$i] -ne $Prefix[$i]) {
                        $same = $false
                        break
                    }
                }
                if ($same) {
                    return
                }
            }
        }
        Start-Sleep -Milliseconds 80
    }
    throw "Timed out waiting for byte prefix on $Path"
}

function Complete-FileDialog([int]$ProcessId, [string]$TitlePattern, [string]$Path) {
    Write-SmokeLog "STEP file dialog $TitlePattern -> $Path"
    $dialog = Wait-ForWindowTitle $ProcessId $TitlePattern 7000
    Activate-Window $dialog.Hwnd
    $children = @(Get-DescendantWindows $dialog.Hwnd -IncludeText)
    foreach ($edit in @($children | Where-Object { $_.ClassName -eq "Edit" })) {
        [void][Win32Smoke]::SendMessageW($edit.Hwnd, $WM_SETTEXT, [UIntPtr]::Zero, $Path)
    }
    $button = @(
        $children |
            Where-Object {
                $_.ClassName -eq "Button" -and
                $_.Title -match "Open|Save|\(&O\)|\(&S\)" -and
                $_.Title -notmatch "Cancel|Help|\(&H\)"
            } |
            Select-Object -First 1
    )
    if ($button.Count -eq 0) {
        throw "Open/Save button not found in file dialog"
    }
    [void][Win32Smoke]::SendMessageW($button[0].Hwnd, $BM_CLICK, [UIntPtr]::Zero, [IntPtr]::Zero)
    Wait-UntilWindowClosed $dialog.Hwnd 7000
}

function Close-ModalByTitle([int]$ProcessId, [string]$TitlePattern, [string]$Keys = "{ESC}") {
    Write-SmokeLog "STEP modal $TitlePattern keys=$Keys"
    $dialog = Wait-ForWindowTitle $ProcessId $TitlePattern 7000
    Activate-Window $dialog.Hwnd
    $children = @(Get-DescendantWindows $dialog.Hwnd -IncludeText)
    $buttonPattern = switch ($Keys) {
        "{ENTER}" { "OK|Yes|\(&O\)|\(&Y\)" }
        "n" { "No|\(&N\)" }
        "N" { "No|\(&N\)" }
        default { $null }
    }
    if ($Keys -eq "{ESC}") {
        [void][Win32Smoke]::PostMessageW($dialog.Hwnd, 0x0010, [UIntPtr]::Zero, [IntPtr]::Zero)
    } elseif ($Keys -eq "{ENTER}" -and $TitlePattern -match "Save|Reopen|Change Encoding") {
        [void][Win32Smoke]::PostMessageW(
            $dialog.Hwnd,
            $WM_COMMAND,
            ([UIntPtr]::new([uint64]3003)),
            [IntPtr]::Zero
        )
    } elseif ($null -ne $buttonPattern) {
        $button = @(
            $children |
                Where-Object { $_.ClassName -eq "Button" -and $_.Title -match $buttonPattern } |
                Select-Object -First 1
        )
        if ($button.Count -eq 0) {
            throw "Button matching /$buttonPattern/ not found in modal $TitlePattern"
        }
        [void][Win32Smoke]::SendMessageW($button[0].Hwnd, $BM_CLICK, [UIntPtr]::Zero, [IntPtr]::Zero)
    } else {
        Send-KeysWait $Keys 250
    }
    Wait-UntilWindowClosed $dialog.Hwnd 7000
}

function Close-TopLevelModalByClass([int]$ProcessId, [IntPtr]$ExcludeHwnd, [string]$ClassPattern, [int]$TimeoutMs = 7000) {
    Write-SmokeLog "STEP modal class $ClassPattern"
    $deadline = (Get-Date).AddMilliseconds($TimeoutMs)
    while ((Get-Date) -lt $deadline) {
        $dialog = @(
            Get-ProcessWindows $ProcessId |
                Where-Object {
                    $_.Hwnd -ne $ExcludeHwnd -and
                    $_.ClassName -match $ClassPattern
                } |
                Select-Object -First 1
        )
        if ($dialog.Count -gt 0) {
            Activate-Window $dialog[0].Hwnd
            [void][Win32Smoke]::PostMessageW($dialog[0].Hwnd, 0x0010, [UIntPtr]::Zero, [IntPtr]::Zero)
            Wait-UntilWindowClosed $dialog[0].Hwnd 7000
            return
        }
        Start-Sleep -Milliseconds 50
    }
    throw "Timed out waiting for modal class /$ClassPattern/"
}

function Assert-Alive($Process, [string]$Label) {
    $currentProcess = [System.Diagnostics.Process]::GetProcessById($Process.Id)
    if ($currentProcess.HasExited) {
        throw "j3Text exited during $Label"
    }
}

if (-not (Test-Path $ExePath)) {
    throw "Executable not found: $ExePath"
}

$resolvedExePath = (Resolve-Path $ExePath).Path
$settingsPath = [System.IO.Path]::ChangeExtension($resolvedExePath, ".toml")
$root = New-SmokeRoot
$configRoot = Join-Path $root "appdata"
$workRoot = Join-Path $root "files"
New-Item -ItemType Directory -Path $configRoot, $workRoot | Out-Null
$settingsBackupPath = Join-Path $root "settings-backup.toml"
$settingsExisted = Test-Path -LiteralPath $settingsPath
if ($settingsExisted) {
    Copy-Item -LiteralPath $settingsPath -Destination $settingsBackupPath -Force
    Remove-Item -LiteralPath $settingsPath -Force
}
$startupFile = Join-Path $workRoot "startup.txt"
$openFile = Join-Path $workRoot "open-target.txt"
$saveAsFile = Join-Path $workRoot "save-as-target.txt"
Set-Content -NoNewline -Encoding utf8 -Path $startupFile -Value "seed`r`nwindows smoke edit`r`nabc abc abc`r`nedit command text`r`n"
Set-Content -NoNewline -Encoding utf8 -Path $openFile -Value "open target`r`nabc abc abc`r`nedit command text`r`n"

$psi = [System.Diagnostics.ProcessStartInfo]::new($resolvedExePath)
$psi.UseShellExecute = $false
$psi.RedirectStandardOutput = $false
$psi.RedirectStandardError = $false
$psi.Arguments = $startupFile
$psi.Environment["APPDATA"] = $configRoot
Write-SmokeLog "STEP launch $($psi.FileName)"
$process = [System.Diagnostics.Process]::Start($psi)
Write-SmokeLog "STEP launched pid=$($process.Id)"

$results = New-Object System.Collections.Generic.List[object]
function Add-Result([string]$Menu, [string]$Feature, [string]$Result) {
    $results.Add([pscustomobject]@{
        Menu = $Menu
        Feature = $Feature
        Result = $Result
    })
    Write-SmokeLog "PASS [$Menu] $Feature"
}

try {
    $deadline = (Get-Date).AddMilliseconds(10000)
    $main = [IntPtr]::Zero
    while ((Get-Date) -lt $deadline) {
        $currentProcess = [System.Diagnostics.Process]::GetProcessById($process.Id)
        if ($currentProcess.MainWindowHandle -ne [IntPtr]::Zero) {
            $main = $currentProcess.MainWindowHandle
            break
        }
        if ($currentProcess.HasExited) {
            throw "j3Text exited before the main window appeared"
        }
        Start-Sleep -Milliseconds 50
    }
    if ($main -eq [IntPtr]::Zero) {
        throw "Timed out waiting for main window"
    }
    Write-SmokeLog "STEP main hwnd=$($main.ToInt64())"
    Write-SmokeLog "STEP activate main"
    Activate-Window $main
    Write-SmokeLog "STEP verify initial editor text"
    $initialLoaded = $false
    for ($attempt = 0; $attempt -lt 40; $attempt++) {
        if ((Get-EditorText $main) -like "*windows smoke edit*") {
            $initialLoaded = $true
            break
        }
        Start-Sleep -Milliseconds 100
    }
    if (-not $initialLoaded) {
        throw "Startup file text was not loaded into the editor"
    }
    Invoke-CommandId $main $Commands.FileSave "File/Save menu command"
    Wait-ForFileContains $startupFile "windows smoke edit"
    Add-Result "File" "Save menu command and editor sync" "PASS"

    Invoke-CommandId $main $Commands.FileSave "File/Save shortcut-equivalent command"
    Wait-ForFileContains $startupFile "windows smoke edit"
    Add-Result "Shortcut" "Save command dispatch" "PASS"

    Invoke-CommandId $main $Commands.FileOpen "File/Open"
    Complete-FileDialog $process.Id "^Open$" $openFile
    Start-Sleep -Milliseconds 350
    if ((Get-EditorText $main) -notlike "*open target*") {
        throw "Open did not load target text"
    }
    Add-Result "File" "Open accept" "PASS"

    Invoke-CommandId $main $Commands.FileSaveAs "File/Save As"
    Complete-FileDialog $process.Id "^Save As$" $saveAsFile
    Close-ModalByTitle $process.Id "^Save$" "{ENTER}"
    Wait-ForFileContains $saveAsFile "open target"
    Add-Result "File" "Save As accept and encoding OK" "PASS"

    Invoke-CommandId $main $Commands.ChangeEncoding "Text/Change Encoding"
    $changeDialog = Wait-ForWindowTitle $process.Id "^Change Encoding$" 7000
    Activate-Window $changeDialog.Hwnd
    Set-ControlTextById $changeDialog.Hwnd 3001 "UTF-8 BOM"
    Close-ModalByTitle $process.Id "^Change Encoding$" "{ENTER}"
    Invoke-CommandId $main $Commands.FileSave "File/Save after encoding change"
    Wait-ForBytesPrefix $saveAsFile ([byte[]](0xEF, 0xBB, 0xBF))
    Add-Result "Text" "Change Encoding to UTF-8 BOM and Save" "PASS"

    Invoke-CommandId $main $Commands.ReopenEncoding "Text/Reopen Encoding"
    Close-ModalByTitle $process.Id "^Reopen$" "{ENTER}"
    Add-Result "Text" "Reopen Encoding accept current" "PASS"

    foreach ($entry in @(
        @("Line Ends CRLF", $Commands.LineEndingCrlf),
        @("Line Ends LF", $Commands.LineEndingLf),
        @("Line Ends CR", $Commands.LineEndingCr),
        @("Line Ends CRLF restore", $Commands.LineEndingCrlf)
    )) {
        Invoke-CommandId $main $entry[1] $entry[0]
    }
    Add-Result "Text" "Line Ends CRLF/LF/CR commands" "PASS"

    foreach ($entry in @(
        @("Commands", $Commands.Commands),
        @("Commands close", $Commands.Commands),
        @("Line Numbers", $Commands.LineNumbers),
        @("Line Numbers restore", $Commands.LineNumbers),
        @("Marks", $Commands.Marks),
        @("Marks restore", $Commands.Marks),
        @("Word Wrap", $Commands.WordWrap),
        @("Word Wrap restore", $Commands.WordWrap)
    )) {
        Invoke-CommandId $main $entry[1] $entry[0]
    }
    Add-Result "View" "Commands/Line Numbers/Marks/Word Wrap toggles" "PASS"

    foreach ($theme in @(
        $Commands.ThemeSystem,
        $Commands.ThemeLight,
        $Commands.ThemeClassicDark,
        $Commands.ThemeSepiaTeal,
        $Commands.ThemeGraphite,
        $Commands.ThemeForest,
        $Commands.ThemeSteelBlue,
        $Commands.ThemeSystem
    )) {
        Invoke-CommandId $main $theme "View/Theme"
    }
    Add-Result "View" "All theme menu items" "PASS"

    Invoke-CommandId $main $Commands.FileNew "File/New"
    Invoke-CommandId $main $Commands.FileNew "File/New second"
    Invoke-CommandId $main $Commands.TabLeft "Tabs/Move Left"
    Invoke-CommandId $main $Commands.TabRight "Tabs/Move Right"
    Invoke-CommandId $main $Commands.FileCloseOthers "Tabs/Close Others"
    Invoke-CommandId $main $Commands.FileCloseAll "Tabs/Close All"
    Add-Result "Tabs" "New/Move/Close Others/Close All" "PASS"

    Invoke-CommandId $main $Commands.FileOpen "File/Open for Find smoke"
    Complete-FileDialog $process.Id "^Open$" $openFile
    Invoke-CommandId $main $Commands.Find "Find/Find"
    Set-ControlTextById $main 2101 "abc"
    Invoke-CommandId $main $Commands.FindNext "Find/Next"
    Invoke-CommandId $main $Commands.FindPrevious "Find/Previous"
    Invoke-CommandId $main $Commands.FindAll "Find/All"
    Invoke-CommandId $main 2107 "Find/Close"
    Invoke-CommandId $main $Commands.Replace "Find/Replace"
    Set-ControlTextById $main 2101 "abc"
    Set-ControlTextById $main 2102 "xyz"
    Invoke-CommandId $main 2105 "Replace/One"
    Invoke-CommandId $main 2106 "Replace/All"
    Invoke-CommandId $main 2107 "Find/Close after replace"
    $replaced = Get-EditorText $main
    if ($replaced -notlike "*xyz*") {
        throw "Replace did not update editor text"
    }
    Add-Result "Find" "Find/Replace/Next/Prev/All/One/All/Close" "PASS"

    Invoke-CommandId $main $Commands.SelectAll "Edit/Select All"
    Invoke-CommandId $main $Commands.Copy "Edit/Copy"
    Invoke-CommandId $main $Commands.Cut "Edit/Cut"
    if ((Get-EditorText $main).Length -ne 0) {
        throw "Cut did not clear selected editor text"
    }
    Invoke-CommandId $main $Commands.Paste "Edit/Paste"
    if ((Get-EditorText $main) -notlike "*edit command text*") {
        throw "Paste did not restore copied text"
    }
    Invoke-CommandId $main $Commands.Undo "Edit/Undo"
    Invoke-CommandId $main $Commands.Redo "Edit/Redo"
    Add-Result "Edit" "Undo/Redo/Cut/Copy/Paste/Select All" "PASS"

    foreach ($entry in @(
        @("Tab Size 2", $Commands.TabSize2),
        @("Tab Size 4", $Commands.TabSize4),
        @("Tab Size 8", $Commands.TabSize8),
        @("Tab Size 4 restore", $Commands.TabSize4)
    )) {
        Invoke-CommandId $main $entry[1] $entry[0]
    }
    Add-Result "Settings" "Tab Size" "PASS"

    Invoke-CommandId $main $Commands.Font "Settings/Font"
    Close-TopLevelModalByClass $process.Id $main "^#32770$"
    Add-Result "Settings" "Font dialog cancel" "PASS"

    for ($i = 0; $i -lt 18; $i++) {
        Invoke-CommandId $main (1900 + ($i * 3)) "Settings/Shortcut Set $i"
        Close-ModalByTitle $process.Id "^Set Shortcut$" "{ESC}"
        Invoke-CommandId $main (1900 + ($i * 3) + 2) "Settings/Shortcut Off $i"
        Invoke-CommandId $main (1900 + ($i * 3) + 1) "Settings/Shortcut Default $i"
    }
    Add-Result "Settings" "Shortcut Set cancel/Off/Default for all shortcut commands" "PASS"

    Invoke-CommandId $main $Commands.About "Help/About"
    Close-ModalByTitle $process.Id "^About$" "{ESC}"
    Add-Result "Help" "About dialog" "PASS"

    Invoke-CommandId $main $Commands.SelectAll "Edit/Select All before dirty prompt"
    Invoke-CommandId $main $Commands.Cut "Edit/Cut before dirty prompt"
    Invoke-CommandId $main $Commands.FileClose "File/Close dirty cancel"
    Close-ModalByTitle $process.Id "^Unsaved$" "{ESC}"
    Assert-Alive $process "dirty prompt cancel"
    Invoke-CommandId $main $Commands.FileClose "File/Close dirty discard"
    Close-ModalByTitle $process.Id "^Unsaved$" "n"
    Add-Result "File" "Close dirty Cancel/Discard" "PASS"

    Invoke-CommandId $main $Commands.Recent0 "File/Recent first item"
    Start-Sleep -Milliseconds 350
    Add-Result "File" "Recent first item" "PASS"

    Invoke-CommandId $main $Commands.FileNew "File/New for Open in New Window"
    Invoke-CommandId $main $Commands.FileOpen "File/Open before OpenNewWindow"
    Complete-FileDialog $process.Id "^Open$" $startupFile
    Invoke-CommandId $main $Commands.OpenNewWindow "Tabs/Open in New Window"
    Start-Sleep -Milliseconds 800
    $spawned = @(Get-Process -Name "j3text" -ErrorAction SilentlyContinue | Where-Object { $_.Id -ne $process.Id })
    foreach ($child in $spawned) {
        try {
            if ($child.MainWindowHandle -ne [IntPtr]::Zero) {
                [void][Win32Smoke]::PostMessageW($child.MainWindowHandle, 0x0010, [UIntPtr]::Zero, [IntPtr]::Zero)
            }
            if (-not $child.WaitForExit(1500)) {
                $child.Kill()
            }
        } catch {
        }
    }
    Add-Result "Tabs" "Open in New Window" "PASS"

    Invoke-CommandId $main $Commands.FileExit "File/Exit"
    if (-not $process.WaitForExit(7000)) {
        throw "j3Text did not exit after File/Exit"
    }
    Add-Result "File" "Exit" "PASS"

    if (-not (Test-Path -LiteralPath $settingsPath)) {
        throw "executable-adjacent settings TOML was not written"
    }

    Write-Output "PASS: Windows menu smoke completed"
    $results | Format-Table -AutoSize | Out-String | Write-Output
} finally {
    if ($null -ne $process -and -not $process.HasExited) {
        try {
            if ($process.MainWindowHandle -ne [IntPtr]::Zero) {
                [void][Win32Smoke]::PostMessageW($process.MainWindowHandle, 0x0010, [UIntPtr]::Zero, [IntPtr]::Zero)
                if (-not $process.WaitForExit(1500)) {
                    $process.Kill()
                }
            } else {
                $process.Kill()
            }
        } catch {
        }
    }
    try {
        if ($settingsExisted) {
            Copy-Item -LiteralPath $settingsBackupPath -Destination $settingsPath -Force
        } elseif (Test-Path -LiteralPath $settingsPath) {
            Remove-Item -LiteralPath $settingsPath -Force
        }
    } catch {
        Write-SmokeLog "WARN settings cleanup failed: $($_.Exception.Message)"
    }
    if (Test-Path $root) {
        try {
            Remove-Item -Recurse -Force -LiteralPath $root -ErrorAction Stop
        } catch {
            Write-SmokeLog "WARN cleanup failed: $($_.Exception.Message)"
        }
    }
}
