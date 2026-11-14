# Using Edolterm

## Tabs and windows

Click **+** for the default profile, or use the dropdown next to it to select
another profile or open settings. Tabs shrink to fit, then scroll horizontally
when the tab bar fills up. Drag to reorder; middle-click to close.

On Windows, drag a tab out of the tab bar to detach it into a new window, or onto
another window's tab bar to attach it. The shell and output remain alive during
the move. Press Esc during a drag to return it to its original window and position.

Right-click a terminal tab to rename, recolor, duplicate, export text, search,
close other tabs, or close tabs to its right. Double-click a tab to rename it;
save an empty name to restore the shell-provided title. Custom names and colors
are included in saved sessions. Export saves the current screen and retained
scrollback as plain UTF-8 text.

Duplicating a tab starts a new shell with the same profile and working directory,
name, and color. It does not clone running commands, output, or environment changes
made inside the original shell. Closing a tab also terminates its shell.

### Window selection

On Windows, one GUI process manages multiple windows. Later launches forward
requests to the running process in the same login session, privilege level, and
configuration directory. Elevated and ordinary instances remain separate.

| `windowing_behavior` | New launches |
| --- | --- |
| `"useExisting"` (default) | Add a tab to the most recently used window on the current virtual desktop, or create a window if none exists. |
| `"useAnyExisting"` | Add a tab to the most recently used window across virtual desktops. |
| `"useNew"` | Create a new window. |

`--new-window` and Ctrl+Shift+N create a new window regardless of this setting.
Settings are shared across windows; the GUI exits when its last window closes.

## Keyboard and mouse

| Action | Default shortcut |
| --- | --- |
| New tab | Ctrl+Shift+T |
| Duplicate tab | Ctrl+Shift+D |
| New window | Ctrl+Shift+N |
| Close tab / window | Ctrl+Shift+W / Alt+F4 |
| Next / previous tab | Ctrl+Tab / Ctrl+Shift+Tab |
| Launch profile 1–9 | Ctrl+Shift+1–9 |
| Profile dropdown | Ctrl+Shift+Space |
| Switch to tab 1–8 / last tab | Ctrl+Alt+1–8 / Ctrl+Alt+9 |
| Settings / settings file | Ctrl+, / Ctrl+Shift+, |
| Copy | Ctrl+Shift+C, Ctrl+Insert, or Enter when text is selected |
| Paste | Ctrl+Shift+V or Shift+Insert |
| Select all, including scrollback | Ctrl+Shift+A |
| Find | Ctrl+Shift+F |
| Next / previous match | Enter / Shift+Enter in search |
| Close search | Esc |
| Scroll up / down | Ctrl+Shift+Up / Ctrl+Shift+Down |
| Scroll one page | Ctrl+Shift+PageUp / Ctrl+Shift+PageDown |
| Scroll to top / bottom | Ctrl+Shift+Home / Ctrl+Shift+End |
| Increase font size | Ctrl+= or Ctrl+Numpad + |
| Decrease font size | Ctrl+- or Ctrl+Numpad - |
| Reset font size | Ctrl+0 |
| Full screen | Alt+Enter or F11 |

The shortcuts page lets you add, change, remove, and reset bindings. Duplicate
key combinations are rejected. Save to apply changes to all windows. Unsupported
Windows Terminal actions remain unbound; see the
[shortcut compatibility notes](windows-terminal-shortcuts.md) for the exact scope.

- **Ctrl+C** goes to the shell; right-click pastes.
- Double-click selects a word; triple-click selects a line; Alt+drag selects a rectangle.
- Hold **Shift** to select text or browse scrollback in applications using mouse reporting.
- **Ctrl+mouse wheel** changes the current tab's font size. Ctrl+0 restores the saved size.
- **Ctrl+hover** highlights a detected URL or OSC 8 link; **Ctrl+click** opens it.
- Drop files or folders from Explorer to insert their paths. Multiple paths,
  spaces, and Unicode are supported; dropping paths does not press Enter.

## Custom profiles and configuration

Open settings with Ctrl+, and the TOML file with Ctrl+Shift+,.
The Windows configuration directory is `%APPDATA%\edolterm`, or the directory
specified by `EDOLTERM_CONFIG_DIR`. Session layouts are saved in `session.json`.

The following example defines a custom PowerShell profile. Update `program` and
`cwd` to paths on your machine:

```toml
default_profile = "dev"
font_family = "Cascadia Mono"
font_size = 15.0
line_height = 1.2
cell_width = 0.6
ctrl_scroll_zoom = true
detect_urls = true
scrollback = 10000
theme = "system"
color_scheme = "One Half Dark"
confirm_close = true
restore_session = true
windowing_behavior = "useExisting"
allow_osc52 = false

[[profiles]]
id = "dev"
name = "Development"
program = 'C:\Program Files\PowerShell\7\pwsh.exe'
args = ["-NoLogo"]
cwd = 'C:\Projects'

[profiles.env]
MY_PROJECT = "development"
```

Executables and arguments are stored separately; paths containing spaces do not
need extra embedded shell quotes. A command-line `--working-directory` overrides
the profile's starting folder. An explicit `--profile` starts with that profile
instead of restoring the previous session on initial launch.

| Setting | Accepted values |
| --- | --- |
| `font_size` | 8–48 |
| `line_height` | 0.5–3.0, multiplied by font size |
| `cell_width` | 0.3–2.0, multiplied by font size |
| `scrollback` | 0–1,000,000 lines |
| `theme` | `"system"`, `"dark"`, `"light"` |
| `color_scheme` | A built-in name, such as `"Campbell"`, `"One Half Dark"`, `"Solarized Light"`, or `"Ubuntu"` |

After manual edits, use **Reload settings** in the settings tab. In the graphical
editor, save to apply changes. Terminal font and color changes apply to open tabs;
profile changes apply when starting a new shell. Invalid settings show an error
without silently overwriting the file.

On first launch, Edolterm discovers profiles only if neither saved discoveries
nor custom profiles exist. Results are stored as `[[discovered_profiles]]` and
reused on subsequent launches. Refresh discovery from settings when you install
another shell. A custom `[[profiles]]` entry with the same ID overrides a discovered
profile. See [profile discovery details](first-run-profile-discovery.md).

OSC 52 clipboard access is disabled by default. When enabled, it only permits
terminal output to write to the clipboard; it does not expose clipboard reads.

## Session restoration

Edolterm saves window positions and sizes, tab order, profile IDs, custom names
and colors, and each tab's last reported working folder. Restoring a session
starts fresh shells; it does not restore previous output or running commands.
If a saved directory no longer exists, the profile's starting folder or home
directory is used instead.

Built-in PowerShell, Windows PowerShell, and CMD profiles report directory changes
automatically. PowerShell's existing prompt is preserved, and profile files are
not modified. Command/script execution profiles do not receive the automatic hook.
Edolterm accepts OSC 9;9 directory reports and local-file OSC 7 URIs. WSL and other
shells need to provide their own reports; default WSL profiles start in the
distribution's home directory.

The selected tab starts first; other restored shells follow once it reports a
ready prompt, or after a fallback timeout. Selecting a waiting tab prioritizes
it, and text typed while it starts is delivered once the shell is ready.

## Windows integration

### Explorer and the taskbar

The installer offers an **Open in Edolterm** Explorer entry independently of
default-terminal registration. Windows 11 uses the primary context menu;
Windows 10 uses its traditional menu. Opening a folder starts the default profile
in that folder and follows the configured window-selection policy.

For source builds and portable folders, the legacy per-user menu can be registered
without administrator privileges:

```powershell
.\scripts\explorer-context-menu.ps1 -ExecutablePath 'C:\Apps\Edolterm\edolterm.exe'

# Remove the legacy menu.
.\scripts\explorer-context-menu.ps1 -Action Uninstall
```

On Windows 11 this legacy entry appears under **Show more options**. Run the script
again with the new path if you move the executable. The distribution bundle
includes the same helper.

The taskbar jump list includes discovered and custom profiles. It refreshes on
startup and when settings are saved or reloaded. Profile icons are taken from
shell executables, with a terminal icon as a fallback.

### Default terminal

Installed builds can register Edolterm as the current user's default terminal on
Windows 11, or Windows 10 22H2 build 19045.3031 and later. Select this option during
installation, or use the Windows default-terminal controls in settings afterward.
The previous choice can be restored from the same controls.

Windows Terminal does not need to be installed. Edolterm bundles a source-built
OpenConsole host with its own COM identities for this integration. Portable ZIPs
do not support default-terminal registration. Existing consoles remain where
they are; programs that explicitly launch `wt.exe` need their own external-terminal
setting changed.

### Updates

Use the update controls in settings to check GitHub Releases, download a new
version, and restart to apply it. Edolterm distinguishes installed and portable
packages and verifies the GitHub-reported SHA-256 digest and file size.

Restarting ends shells in all windows, then restores saved window and tab layouts
and folders. Running commands and output are not restored. Installation stops if
another Edolterm process is still running.

See the [Windows packaging guide](../packaging/windows/README.md) for integration
requirements, signing, registration, uninstallation, and troubleshooting.
