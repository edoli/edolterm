# Windows x64 installer and Explorer integration

`ExplorerCommand.cpp` implements `IExplorerCommand` and `IObjectWithSite` in a
small native DLL. Windows activates it in a COM surrogate. Selected filesystem
folders and folder backgrounds resolve to a directory; files, virtual folders,
and multiple selections are hidden. The executable is launched directly with
`--working-directory`, with Windows argument escaping and no intermediary shell.

The Sparse MSIX references `Edolterm.ExplorerHost.exe`, the source-built
`Edolterm.OpenConsole.exe`, and the command DLL in the Inno installation's
`shell-integration` subdirectory. Both hidden EXE endpoints receive package
identity; COM command DLLs run in Windows' `dllhost.exe`. Explicit package
activations use the endpoint to launch the
ordinary terminal executable. `edolterm.exe`
retains its ordinary Win32 identity and embedded ICO, including in the portable
ZIP. The package's external content location is only `shell-integration`, excluding
the GUI and ordinary ConPTY runtime from the package layout. Both launch paths point
to `..\edolterm.exe`, outside that location.
The shell package has its own identity, `Edoli.Edolterm.ShellIntegration`, replacing
the old `Edoli.Edolterm.Explorer` identity shared with the terminal. Upgrade uses
the old installation's helper to unregister the old package before installing
the new one. Pins that explicitly reference the old package AUMID must be pinned
again; ordinary executable shortcuts continue to point to the same terminal.
First-time activation of the extension therefore cannot take over the
terminal's package activation. The Cargo build output is not modified by packaging.
The modern menu schema
supports `Directory` and `Directory\Background`; drive roots work through their
folder background. The old registry script remains available for legacy menus.

## Windows default terminal

Windows 11 users and Windows 10 22H2 users on build 19045.3031+ can choose
**Windows 기본 터미널 → 기본 터미널로 설정**
in Edolterm settings after installing Explorer/Windows integration. This registers
Edolterm as a candidate; installation itself never changes the selected default.
Windows Terminal does not need to be installed. Windows calls the bundled
`Edolterm.OpenConsole.exe` through `IConsoleHandoff`, and OpenConsole invokes
Edolterm's `ITerminalHandoff3` endpoint in `dllhost.exe`. Our RPC proxy marshals
the Windows handle ABI. Both COM classes belong to Edolterm.

`scripts/build-openconsole.ps1` downloads the hash-verified Microsoft Terminal
v1.25.2733.0 source archive and builds only `Host.EXE.vcxproj` and its dependencies.
It changes the console handoff CLSID to `93C0413C-F8D7-49D7-8C07-62E4D1039ED4` and
uses the VS 2022 v143 dependency triplet. The console implementation stays upstream.
The source, dependencies, and build outputs are cached under
`target/openconsole-source`; variants share libraries under an exclusive build
lock. `openconsole-build.json` records the source hash, modification, class ID,
and executable hash before identity embedding/signing. Packaging signs the EXE
with Edolterm's existing certificate. The ordinary NuGet ConPTY pair remains in
the app root for ordinary tabs and portable updates; this additional console
host belongs to the installed Windows integration only.
The upstream MIT license and third-party notices are installed alongside it.

The endpoint passes only this connection's handles to the unpackaged GUI using
an explicit inheritance list. A running GUI duplicates handles from the verified
named-pipe client PID before acknowledging forwarding. IPC version 2 keeps older
GUIs from interpreting handoffs as ordinary shell launches. PTYs consume the
existing console, rather than rerunning the command, and wait for output EOF after
the last attached client exits. Window selection follows the configured policy.

`shell-integration/windows-default-terminal.ps1 -Action Set|Restore|Status` provides
the same setting outside the UI. It preserves the previous delegation pair and
uninstall restores it only while Edolterm is selected. Updates preserve the current
choice; removing integration restores the prior choice. Portable ZIPs and Windows
10 retain their existing launch/Explorer behavior without default-terminal support.

`scripts/test-windows-default-terminal.ps1` verifies a real OS console launch through
both COM handoffs, input/output, resize, and exit using an isolated signed package.
It verifies the server process handle resolves to the fixture's source-built
OpenConsole, proving an installed Windows Terminal is not supplying the console.
The v3 handoff also verifies a Unicode/quoted title.
The script builds the separate `handoff_probe` example, which uses the production
Session/PTY without adding test entry points to the GUI. The test requires the
installer payload and an already trusted development signing key; tests never
import certificates. A test-only Automation object handles
the COM host's registry context, saves/restores its exact previous values, and
restores on release. The fixture accepts only its suspended test client's PID;
unrelated console launches never receive probe input. No test control is shipped.

## Supported Windows versions

The application and Inno installer target Windows 10 1809 (17763) or later, x64 only.
Default-terminal registration supports all released Windows 11 versions and
Windows 10 22H2 build 19045.3031+ (KB5026435 or a superseding cumulative update),
following [Microsoft's supported OS range](https://learn.microsoft.com/windows/terminal/install#set-your-default-terminal-application).
`windows-terminal-support.ps1` shares this decision between registration and
installation; `test-windows-terminal-support.ps1` checks its version boundaries.
The sparse-package minimum is build 19041, required by external-location identity;
the helpers enforce the additional default-terminal OS/update requirements.
Its Explorer context-menu schema is available on Windows 10; Windows 11 presents
the command in its modern menu. Older Windows 10 builds retain registry menus and
ordinary ConPTY tabs without sparse-package registration or certificate changes.

The bundled ConPTY runtime supports Windows 10 1809. Runtime checks performed in
this workspace use the available Windows 11 host. Windows 10 and Windows 11 21H2
execution still require validation on those systems; version-boundary tests and
the documented API minimums do not substitute for that runtime validation.

## Build

Requirements: Rust MSVC, Visual Studio 2022 C++ tools (v143), CMake, Windows SDK
10.0.26100 (`mt.exe`, `MakeAppx.exe`, `SignTool.exe`), and Python 3.11+.
`scripts/install-inno.ps1` prepares the pinned, hash-checked Inno Setup 6.7.3.

```powershell
.\scripts\build-windows-installer.ps1
# Reuse an already built application:
.\scripts\build-windows-installer.ps1 -SkipAppBuild
# Explicit Inno compiler location:
.\scripts\build-windows-installer.ps1 -InnoCompiler 'C:\Tools\Inno Setup 6\ISCC.exe'
```

Output: `dist/windows/edolterm-installer-exe.exe` and `edolterm-windows.zip`.
`package-checksums-windows.json` records final signed artifact hashes and sizes.
The portable ZIP contains the app, both ConPTY runtime binaries, license, runtime
hash metadata, and legacy menu helper. Packaging verifies the exact set and x64 PE headers.

`release` uses moderate optimization for local iteration. `release-dist` is the
fully optimized distribution profile and the packaging default. `-Profile release`
is available for faster installer iteration. `Cargo.toml` is the version source;
packaging generates the MSIX/Inno versions and rejects a stale executable or a
mismatched `v<version>` tag. The `payload` subdirectory also
contains the signed application, DLL, Sparse MSIX, public certificate, and
registration script. Native integration tests are optional and do not run during packaging.

The first build creates a non-exportable RSA development signing key in the
builder's `CurrentUser\My` certificate store. Only the public `.cer` is shipped;
no PFX or private key is written to the payload. The thumbprint in the ignored
`target/signing/development-thumbprint.txt` lets later builds reuse the same key.
Keep that signing identity for updates. All package/executable publisher metadata
must match the certificate subject. Change the version only in `Cargo.toml` for a release; `AppxManifest.xml` is a template. This is a development signing configuration, not
a publicly trusted publisher identity. Downloaded installers may show SmartScreen.

## Install and uninstall

Inno offers current-user (default, `%LOCALAPPDATA%\Programs\Edolterm`) and
all-users (`%ProgramFiles%\Edolterm`) installation. Only all-users installation
requires elevation of the installer. The Explorer task is selected by default.
On supported default-terminal OS versions the certificate helper imports only the public certificate into
`LocalMachine\TrustedPeople`; canceling UAC reports integration failure.
Current-user registration uses `Add-AppxPackage -ExternalLocation`. All-users
installation stages and provisions the package for users on sign-in, and registers
it immediately for the installing account. Older Windows 10 builds use HKCU/HKLM
registry verbs according to scope, with no certificate registration.

Setup checks write access to the existing executable before changing integration
or application files. If in use, it stops and asks the user to close every window.
`CloseApplications=no` prevents forced shutdown or Restart Manager intervention.
Uninstall retains user settings/session data and shared certificate trust.

The signed MSIX, app, DLL, and certificate must be kept together. Manual registration:

```powershell
powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\shell-integration\windows-shell-integration.ps1 -Action Install
powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\shell-integration\windows-shell-integration.ps1 -Action Uninstall
```

Successful registration removes this app's older registry verbs, avoiding duplicate
commands. If Explorer caches an old extension, close and reopen its windows or
sign out/in. The installer never force-restarts Explorer or terminates terminals.

Uninstall unregisters the selected scope before removing files; an all-users uninstall also removes provisioning.
Machine-wide certificate trust is retained because another user may still use it.
To remove that trust after all users have uninstalled the integration, run the
following against a retained payload directory in an administrator PowerShell:

```powershell
.\shell-integration\windows-shell-integration.ps1 -Action RemoveCertificate
```

## Optional validation

These tests are excluded from normal packaging and CI. The `tests` directory and
the two test scripts need only be present when running the checks below.

```powershell
cmake -S packaging/windows -B target/windows-tests/native -A x64 -DBUILD_TESTING=ON
cmake --build target/windows-tests/native --config Release
ctest --test-dir target/windows-tests/native -C Release --output-on-failure
.\scripts\test-windows-installer.ps1
.\scripts\test-windows-updater.ps1
.\scripts\test-windows-shell-activation.ps1
```

The native tests load the real DLL and exercise folder/background/drive paths,
Unicode and shell metacharacters, visibility, quoting, and COM object lifetime,
using a test executable that records arguments instead of opening a terminal.
The installer smoke test installs into an isolated directory with the Explorer
task disabled (no certificate trust changes), verifies installed files and signer
identity, verifies that a locked executable blocks reinstallation, and uninstalls.
It builds a separate test AppId/Start menu group/updater registry key, preserving
the real installation and its Explorer package.

The shell activation regression test requires Windows 11 and the payload's
signing key plus its already trusted certificate. It never imports certificates.
It creates a unique package name, COM CLSID, Inno identity, registry verbs, and
configuration directory, and uses real AppX registration and COM activation.
It verifies cold/repeated menu queries with a running terminal, directory launch
into an existing GUI process, silent upgrade and registration retention, launch
with the terminal stopped, and package-endpoint launch. Every
terminal process must remain unpackaged. It then uninstalls the test integration
and verifies the production package registration is unchanged. Test-only DLL
instrumentation routes launches to the private config and records child identity.

References:
- https://learn.microsoft.com/windows/apps/desktop/modernize/integrate-packaged-app-with-file-explorer
- https://learn.microsoft.com/windows/apps/desktop/modernize/grant-identity-to-nonpackaged-apps

## GitHub Actions and signing

`.github/workflows/ci.yml` builds Windows x64 only, runs Rust tests, prepares
Inno, validates versions and package hashes, and uploads both artifacts. A `v*` tag
publishes a GitHub Release after checking the exact artifact set and hashes.
Publishing an older version does not replace the latest release.

PR/manual/branch builds may create a temporary development certificate. Tagged
releases require a **stable** code-signing identity in repository Actions secrets:

- `EDOLTERM_SIGNING_PFX_BASE64`: base64 of a password-protected signing PFX.
- `EDOLTERM_SIGNING_PASSWORD`: that PFX password.

Its subject must be `CN=Edolterm Development`. Keep the same private key for later
updates. The existing local non-exportable development key cannot be exported for
CI; create a separate exportable development signing key once, store its PFX in
Actions secrets, and keep a secure backup outside the repository. Changing the
certificate identity requires reinstalling/trusting the new publisher. Never
commit the PFX, password, or private key or include them in artifacts. This task
does not configure GitHub secrets or publish a release.

For a local PFX build use `-SigningPfxPath` and the `EDOLTERM_SIGNING_PASSWORD`
environment variable. Local builds without a PFX reuse the cached store certificate.
The update repository is `-Repository owner/repository` (default `edoli/edolterm`);
Actions supplies its own repository. The current local Git repository has no remote,
so configure the actual hosting repository before publishing.

## In-app updates

The settings tab checks GitHub Releases explicitly. It chooses the installer when
its directory matches the selected-scope installation registry; otherwise it chooses
the multi-file portable ZIP. Downloads are bounded and verified using the SHA-256
and size from GitHub release asset metadata. An isolated helper verifies the hash
again before acknowledging readiness, then waits for an explicit apply request and
normal application exit. The app saves its session and shuts down its PTYs first.
The helper never kills the application or another terminal window. All-users updates
request elevation for Setup only; restart runs as the original user. Portable updates
validate the exact flat ZIP payload, stage and back up every managed file, and roll
back a failed copy. Settings and other files are left intact. Failures survive restart
in the configuration directory's `update-result.txt`.

Integration logs are under `%LOCALAPPDATA%\Edolterm\Logs`; installer logs are
available via Inno's `/LOG` option. The installed Explorer marker ensures uninstall
without the menu task cannot remove another installation's menu registration.
