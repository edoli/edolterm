# Developing Edolterm

## Build and run

The current distribution target is Windows x64. Install a Rust MSVC toolchain,
Visual Studio C++ Build Tools, and the Windows SDK. Release builds also require
`fxc.exe` for GPUI shader compilation; `build.rs` searches standard SDK locations,
or you can set `GPUI_FXC_PATH` explicitly.

```powershell
cargo run --locked --release
```

Both `gpui` and `gpui_platform` are pinned to Zed revision
`b5cd310b3b54dfc659fcc06453e6358843dfe216`. Update them together and retain
`Cargo.lock` for reproducible dependency resolution. `alacritty_terminal` 0.26
is vendored under `vendor/alacritty_terminal`; local changes are described in
[its integration notes](../vendor/alacritty_terminal/EDOLTERM.md).

### ConPTY runtime

Windows Cargo builds download and SHA-256-verify Microsoft's ConPTY package
`1.25.260930003`, then place the runtime next to the executable. This applies to
debug and release builds, tests, and examples, including after `cargo clean`.
The first build needs a network connection; later builds use the cache.

Keep `edolterm.exe`, `conpty.dll`, and `OpenConsole.exe` in the same directory.
Do not replace system ConPTY files. The Windows I/O adapter addresses an
Alacritty 0.26 integration issue in which read notifications can stall during
heavy output; see [the ConPTY investigation](../CONPTY_FIX.md).

## Distribution builds

| Cargo profile | Purpose |
| --- | --- |
| `release` | Iteration: optimization level 2, no LTO. |
| `release-dist` | Distribution: optimization level 3, thin LTO, one codegen unit. |

Build a complete standalone folder with runtime information and licenses:

```powershell
cargo build --locked --profile release-dist
.\scripts\bundle-windows.ps1 -Profile release-dist -OutputDir dist/edolterm
```

To build the installer and portable ZIP:

```powershell
.\scripts\build-windows-installer.ps1
```

The outputs are `dist/windows/edolterm-installer-exe.exe` and
`dist/windows/edolterm-windows.zip`. Installer builds have additional requirements,
including the VS 2022 v143 toolset, Windows SDK packaging/signing tools, CMake,
Python, and Inno Setup. See the [packaging guide](../packaging/windows/README.md)
before building or testing Windows integration.

The package version comes from `Cargo.toml` and is shared by the executable,
Inno installer, and Sparse MSIX. Release tags must use `v<version>`. CI uses
`release-dist`. Set `EDOLTERM_RELEASE_REPOSITORY` at build time to change the
updater's source repository; the default is `edoli/edolterm`, and Actions uses
the current GitHub repository.

## Validation

Run the Rust checks from the repository root:

```powershell
cargo fmt --check
cargo test --locked -- --test-threads=1
cargo clippy --all-targets --features ui-tests -- -D warnings
```

Integration tests launch real PowerShell/CMD sessions through Windows ConPTY to
check Unicode, input, resize, exit cleanup, and working-directory restoration.

The in-process UI smoke suite uses separate directories under `target/ui-smoke`;
it does not inject OS keyboard or mouse events or use your normal configuration.

```powershell
# Build with UI test support and run all smoke modes.
.\scripts\smoke.ps1

# Run one mode; the script builds the required feature automatically.
.\scripts\smoke.ps1 -Mode settings
.\scripts\smoke.ps1 -Mode scroll
.\scripts\smoke.ps1 -Mode instances
```

Modes include `single`, `terminal`, `menu`, `settings`, `overflow`, `find`,
`confirm`, `scroll`, `drop`, `tabs`, `instances`, `keys`, `shortcuts`, `layout`,
`links`, and `updates`. Results and GPU-rendered screenshots are saved in each
mode's directory. The runner requests graceful shutdown using a `stop` file.
Smoke-test entry points are excluded from release builds.

The `instances` mode checks real subprocess forwarding, window-selection policies,
shared settings, window lifecycle, and session restoration. The `scroll` mode
checks burst input, reversal, and idle position; `drop` checks quoted and Unicode
paths delivered to a real PowerShell PTY without executing them.

OS integration checks are separate scripts under `scripts/test-windows-*.ps1`.
Some require an installer payload and an already trusted signing key. Read the
packaging guide before running them. Windows 10 and older Windows 11 releases
still need runtime validation on those systems; version-boundary tests do not replace it.

## Implementation notes

- [User guide](usage.md): profiles, shortcuts, session semantics, and Windows integration.
- [First-run profile discovery](first-run-profile-discovery.md): cached discovery and explicit refresh.
- [Background startup](startup-background.md): asynchronous shell creation and restoration.
- [Settings UI](settings-ui.md): controls, lazy font discovery, and startup measurements.
- [Terminal layout](terminal-layout.md): font metrics and physical-pixel alignment.
- [Shortcut compatibility](windows-terminal-shortcuts.md): supported and unbound actions.
- [Vendored terminal core](../vendor/alacritty_terminal/EDOLTERM.md): local Alacritty changes.

Some investigation notes are in Korean. They describe specific measurements and
validation environments, rather than guarantees for every machine.

The application icon source is `assets/app-icon/edolterm-icon.svg`. Windows builds
generate a multi-resolution ICO and embed it in the executable. UI SVG assets are
embedded as well. Taskbar profile icons are extracted in the background and cached
under the configuration directory's `cache` folder.

## README screenshots

See [screenshot provenance and reproduction](screenshots/README.md). The terminal
scene is generated by `scripts/readme-demo.ps1` from actual checkout contents;
the settings image is captured by the UI smoke suite. Run
`scripts/capture-readme.ps1` to build and launch the isolated debug capture mode,
verify default tab colors, and save the main image at native GPU resolution.
