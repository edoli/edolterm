// A separate package endpoint keeps COM activation away from the GPUI process.
#define WIN32_LEAN_AND_MEAN
#define NOMINMAX
#include <windows.h>
#include <wrl.h>
#include <atomic>
#include <filesystem>
#include <fstream>
#include <string>
#include <utility>
#include <vector>
#include "TerminalHandoff.h"

using namespace Microsoft::WRL;
namespace {
HMODULE module{};
std::atomic<unsigned> objects{};
void log_failure(const char* stage, HRESULT result) noexcept {
    try {
        wchar_t directory[32768]{};
        const auto configured = GetEnvironmentVariableW(L"EDOLTERM_CONFIG_DIR", directory, 32768);
        const auto length = configured ? configured : GetEnvironmentVariableW(L"LOCALAPPDATA", directory, 32768);
        if (!length || length >= 32768) return;
        const auto logs = configured ? std::filesystem::path(directory) / L"Logs" : std::filesystem::path(directory) / L"Edolterm" / L"Logs";
        std::filesystem::create_directories(logs);
        std::ofstream(logs / L"terminal-handoff.log", std::ios::app)
            << GetTickCount64() << " " << GetCurrentProcessId() << " " << stage << " 0x" << std::hex << result << '\n';
    } catch (...) {}
}
struct Handle {
    HANDLE value{};
    Handle() = default;
    explicit Handle(HANDLE h) : value(h) {}
    Handle(const Handle&) = delete;
    Handle& operator=(const Handle&) = delete;
    Handle(Handle&& other) noexcept : value(std::exchange(other.value, nullptr)) {}
    ~Handle() { if (value && value != INVALID_HANDLE_VALUE) CloseHandle(value); }
    HANDLE release() { return std::exchange(value, nullptr); }
};
HRESULT duplicate(HANDLE source, Handle& result, BOOL inherit = FALSE) {
    return DuplicateHandle(GetCurrentProcess(), source, GetCurrentProcess(), &result.value,
                           0, inherit, DUPLICATE_SAME_ACCESS) ? S_OK : HRESULT_FROM_WIN32(GetLastError());
}
std::wstring quote(const std::wstring& value) {
    std::wstring out = L"\"";
    size_t slashes = 0;
    for (const auto c : value) {
        if (c == L'\\') { ++slashes; continue; }
        out.append(slashes * (c == L'\"' ? 2 : 1), L'\\');
        if (c == L'\"') out += L'\\';
        out += c;
        slashes = 0;
    }
    out.append(slashes * 2, L'\\');
    return out + L'\"';
}
HRESULT launch(HANDLE input, HANDLE output, HANDLE signal, HANDLE reference,
               HANDLE server, HANDLE client, const TERMINAL_STARTUP_INFO& info) {
    std::wstring path(32768, L'\0');
    const DWORD length = GetModuleFileNameW(module, path.data(), static_cast<DWORD>(path.size()));
    if (!length || length >= path.size()) return HRESULT_FROM_WIN32(ERROR_BAD_PATHNAME);
    path.resize(length);
    const auto directory = std::filesystem::path(path).parent_path().parent_path();
    const auto executable = (directory / L"edolterm.exe").wstring();
    Handle ready(CreateEventW(nullptr, TRUE, FALSE, nullptr));
    if (!ready.value) return HRESULT_FROM_WIN32(GetLastError());
    // Duplicate only the seven handles used by this connection. Never inherit
    // COM/RPC handles or another connection's pipes in concurrent callbacks.
    const HANDLE sources[] = {input, output, signal, reference, server, client, ready.value};
    std::vector<Handle> owned;
    std::vector<HANDLE> inherited;
    std::wstring packet;
    for (const auto source : sources) {
        Handle handle;
        const auto hr = duplicate(source, handle, TRUE);
        if (FAILED(hr)) return hr;
        if (!packet.empty()) packet += L',';
        packet += std::to_wstring(reinterpret_cast<uintptr_t>(handle.value));
        inherited.push_back(handle.value);
        owned.push_back(std::move(handle));
    }
    std::wstring title = L"외부 콘솔";
    if (info.pszTitle) title.assign(info.pszTitle, SysStringLen(info.pszTitle));
    if (title.size() > 8192) title.resize(8192);
    std::wstring arguments = quote(executable) + L" --terminal-handoff=" + packet + L" " + quote(L"--handoff-title=" + title);
    SIZE_T bytes = 0;
    InitializeProcThreadAttributeList(nullptr, 1, 0, &bytes);
    std::vector<unsigned char> storage(bytes);
    auto attributes = reinterpret_cast<PPROC_THREAD_ATTRIBUTE_LIST>(storage.data());
    if (!InitializeProcThreadAttributeList(attributes, 1, 0, &bytes)) return HRESULT_FROM_WIN32(GetLastError());
    struct Attributes { PPROC_THREAD_ATTRIBUTE_LIST value; ~Attributes() { DeleteProcThreadAttributeList(value); } } cleanup{attributes};
    if (!UpdateProcThreadAttribute(attributes, 0, PROC_THREAD_ATTRIBUTE_HANDLE_LIST,
                                   inherited.data(), inherited.size() * sizeof(HANDLE), nullptr, nullptr))
        return HRESULT_FROM_WIN32(GetLastError());
    STARTUPINFOEXW startup{};
    startup.StartupInfo.cb = sizeof(startup);
    startup.StartupInfo.dwFlags = STARTF_USESHOWWINDOW;
    startup.StartupInfo.wShowWindow = SW_SHOWNORMAL;
    startup.lpAttributeList = attributes;
    PROCESS_INFORMATION process{};
    if (!CreateProcessW(executable.c_str(), arguments.data(), nullptr, nullptr, TRUE,
                        EXTENDED_STARTUPINFO_PRESENT | CREATE_UNICODE_ENVIRONMENT | CREATE_NO_WINDOW,
                        nullptr, directory.c_str(), &startup.StartupInfo, &process))
    { const auto error = HRESULT_FROM_WIN32(GetLastError()); log_failure("launch", error); return error; }
    Handle child(process.hProcess), thread(process.hThread);
    // A forwarding launcher can exit before the GUI attaches. The ready event
    // belongs to the receiving PTY and is signalled only once its IO is live.
    owned.clear();
    const auto wait = WaitForSingleObject(ready.value, 30000);
    const auto result = wait == WAIT_OBJECT_0 ? S_OK : HRESULT_FROM_WIN32(wait == WAIT_TIMEOUT ? ERROR_TIMEOUT : GetLastError());
    if (FAILED(result)) log_failure("ready", result);
    return result;
}

class TerminalHandoff final : public RuntimeClass<RuntimeClassFlags<ClassicCom>, ITerminalHandoff3, IDefaultTerminalMarker> {
public:
    TerminalHandoff() { ++objects; }
    ~TerminalHandoff() { --objects; }
    HRESULT STDMETHODCALLTYPE EstablishPtyHandoff(HANDLE* in, HANDLE* out, HANDLE signal, HANDLE reference,
                                                  HANDLE server, HANDLE client, const TERMINAL_STARTUP_INFO* info) noexcept override {
        if (!in || !out || !info) return E_POINTER;
        *in = nullptr; *out = nullptr;
        try {
            Handle inputRead, inputWrite, outputRead, outputWrite;
            if (!CreatePipe(&inputRead.value, &inputWrite.value, nullptr, 128 * 1024) ||
                !CreatePipe(&outputRead.value, &outputWrite.value, nullptr, 128 * 1024))
                return HRESULT_FROM_WIN32(GetLastError());
            const auto hr = launch(inputWrite.value, outputRead.value, signal, reference, server, client, *info);
            if (FAILED(hr)) return hr;
            *in = inputRead.release(); *out = outputWrite.release();
            return S_OK;
        } catch (...) { return E_FAIL; }
    }
};
class Factory final : public RuntimeClass<RuntimeClassFlags<ClassicCom>, IClassFactory> {
public:
    Factory() { ++objects; }
    ~Factory() { --objects; }
    HRESULT STDMETHODCALLTYPE CreateInstance(IUnknown* outer, REFIID iid, void** result) noexcept override {
        if (!result) return E_POINTER;
        *result = nullptr;
        if (outer) return CLASS_E_NOAGGREGATION;
        const auto instance = Make<TerminalHandoff>();
        return instance ? instance->QueryInterface(iid, result) : E_OUTOFMEMORY;
    }
    HRESULT STDMETHODCALLTYPE LockServer(BOOL lock) noexcept override {
        if (lock) ++objects; else --objects;
        return S_OK;
    }
};
}
BOOL WINAPI DllMain(HINSTANCE instance, DWORD reason, LPVOID) {
    if (reason == DLL_PROCESS_ATTACH) { module = instance; DisableThreadLibraryCalls(instance); }
    return TRUE;
}
extern "C" HRESULT __stdcall DllGetClassObject(REFCLSID clsid, REFIID iid, void** result) {
    if (!result) return E_POINTER;
    *result = nullptr;
    if (clsid != CLSID_EdoltermTerminal) return CLASS_E_CLASSNOTAVAILABLE;
    const auto factory = Make<Factory>();
    return factory ? factory->QueryInterface(iid, result) : E_OUTOFMEMORY;
}
extern "C" HRESULT __stdcall DllCanUnloadNow() { return objects == 0 ? S_OK : S_FALSE; }
