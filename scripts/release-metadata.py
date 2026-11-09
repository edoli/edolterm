"""Validate Cargo/tag versions and the two Windows x64 release artifacts."""
import hashlib
import json
import os
from pathlib import Path
import re
import struct
import sys
import tomllib
import zipfile

ROOT = Path(__file__).resolve().parents[1]
ASSETS = ("edolterm-installer-exe.exe", "edolterm-windows.zip")
PORTABLE = {"edolterm.exe", "conpty.dll", "OpenConsole.exe", "LICENSE-ConPTY.txt", "runtime.json", "explorer-context-menu.ps1"}

def version():
    value = tomllib.loads((ROOT / "Cargo.toml").read_text())["package"]["version"]
    assert re.fullmatch(r"(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)", value), "Expected major.minor.patch"
    assert all(int(n) <= 65535 for n in value.split(".")), "Windows version component exceeds 65535"
    if os.environ.get("GITHUB_REF_TYPE") == "tag":
        assert os.environ.get("GITHUB_REF_NAME") == "v" + value, "Release tag must match Cargo.toml"
    return value

def sha(path):
    with path.open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()

def x64(data):
    assert data[:2] == b"MZ", "Missing PE header"
    offset = struct.unpack_from("<I", data, 0x3c)[0]
    assert data[offset:offset + 4] == b"PE\0\0" and struct.unpack_from("<H", data, offset + 4)[0] == 0x8664, "Expected Windows x64 PE"

def record(directory):
    files = {name: {"sha256": sha(directory / name), "size": (directory / name).stat().st_size} for name in ASSETS}
    (directory / "package-checksums-windows.json").write_text(json.dumps({"version": version(), "files": files}, indent=2) + "\n")

def check(directory):
    metadata = json.loads((directory / "package-checksums-windows.json").read_text())
    assert metadata["version"] == version(), "Package version differs from Cargo.toml"
    assert set(metadata["files"]) == set(ASSETS), "Release artifact set mismatch"
    for name in ASSETS:
        path = directory / name
        assert metadata["files"][name] == {"sha256": sha(path), "size": path.stat().st_size}, f"Hash/size mismatch: {name}"
    with zipfile.ZipFile(directory / ASSETS[1]) as package:
        assert len(package.namelist()) == len(PORTABLE) and set(package.namelist()) == PORTABLE, "Portable ZIP contents mismatch"
        for name in ("edolterm.exe", "conpty.dll", "OpenConsole.exe"):
            x64(package.read(name))
        runtime = json.loads(package.read("runtime.json").decode("utf-8-sig"))
        for item in runtime["files"]:
            assert hashlib.sha256(package.read(item["name"])).hexdigest() == item["Hash"].lower(), "Portable runtime hash mismatch"
    print("PASS: Windows x64 package contents, version and SHA-256")

if __name__ == "__main__":
    try:
        command = sys.argv[1]
        if command == "version":
            print(version())
        elif command == "record":
            record(Path(sys.argv[2]))
        elif command == "check":
            check(Path(sys.argv[2]))
        elif command == "publish":
            directory = Path(sys.argv[2])
            check(directory)
            assert {p.name for p in directory.iterdir() if p.is_file()} == set(ASSETS) | {"package-checksums-windows.json"}, "Unexpected release files"
        else:
            raise ValueError("Unknown command")
    except (AssertionError, ValueError, KeyError) as error:
        sys.exit(str(error))
