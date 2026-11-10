// Identity endpoint for Explorer integration. COM runs in dllhost.exe. Explicit
// package activations launch the plain EXE.
#define WIN32_LEAN_AND_MEAN
#include <windows.h>
#include <filesystem>
#include <string>
#include "LaunchTerminal.h"

int WINAPI wWinMain(HINSTANCE, HINSTANCE, PWSTR commandLine, int) {
    try {
        std::wstring path(32768, L'\0');
        const DWORD length = GetModuleFileNameW(nullptr, path.data(), static_cast<DWORD>(path.size()));
        if (!length || length >= path.size()) return ERROR_BAD_PATHNAME;
        path.resize(length);
        const auto directory = std::filesystem::path(path).parent_path().parent_path();
        const auto terminal = (directory / L"edolterm.exe").wstring();
        std::wstring arguments = L"\"" + terminal + L"\"";
        if (commandLine && *commandLine) arguments += L" " + std::wstring(commandLine);
        PROCESS_INFORMATION process{};
        if (!launch_terminal(terminal, arguments,
                            directory.c_str(), process))
            return static_cast<int>(GetLastError());
        CloseHandle(process.hThread);
        CloseHandle(process.hProcess);
        return 0;
    } catch (...) { return ERROR_UNHANDLED_EXCEPTION; }
}
