#pragma once
#include <windows.h>
#include <string>

// The terminal lives outside the shell package's external content directory.
// Launch it directly without a shell or inherited handles.
inline BOOL launch_terminal(const std::wstring& executable, std::wstring& arguments,
                            const wchar_t* directory, PROCESS_INFORMATION& process) {
    STARTUPINFOW startup{sizeof(startup)};
    startup.dwFlags = STARTF_USESHOWWINDOW;
    startup.wShowWindow = SW_SHOWNORMAL;
    return CreateProcessW(executable.c_str(), arguments.data(), nullptr, nullptr, FALSE,
                          CREATE_UNICODE_ENVIRONMENT, nullptr, directory, &startup, &process);
}
