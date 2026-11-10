// Edolterm's Windows 11 Explorer command. Built as an isolated COM surrogate DLL.
#define WIN32_LEAN_AND_MEAN
#define NOMINMAX
#include <windows.h>
#include <shlobj.h>
#include <shlwapi.h>
#include <wrl/client.h>
#include <atomic>
#include <filesystem>
#include <new>
#include <string>
#include "LaunchTerminal.h"

using Microsoft::WRL::ComPtr;
static HMODULE module;
static std::atomic<long> objects{0};
// Keep this GUID in sync with AppxManifest.xml.
static const CLSID commandId = {0x6c14652e, 0xd151, 0x48c8, {0xa7, 0xa5, 0x2f, 0xa4, 0xf5, 0x23, 0x38, 0x97}};

static std::wstring quote(const std::wstring& value) {
    std::wstring result = L"\"";
    size_t slashes = 0;
    for (wchar_t c : value) {
        if (c == L'\\') { ++slashes; continue; }
        result.append(c == L'"' ? slashes * 2 + 1 : slashes, L'\\');
        result += c;
        slashes = 0;
    }
    result.append(slashes * 2, L'\\');
    return result + L'"';
}

static std::filesystem::path executable() {
    std::wstring path(32768, L'\0');
    DWORD count = GetModuleFileNameW(module, path.data(), static_cast<DWORD>(path.size()));
    if (!count || count >= path.size()) throw std::runtime_error("module path");
    path.resize(count);
    return std::filesystem::path(path).parent_path().parent_path() / L"edolterm.exe";
}

class Command final : public IExplorerCommand, public IObjectWithSite {
    std::atomic<ULONG> references{1};
    ComPtr<IUnknown> site;
    HRESULT location(IShellItemArray* selection, IShellItem** result) {
        *result = nullptr;
        if (selection) {
            DWORD count = 0;
            HRESULT hr = selection->GetCount(&count);
            if (FAILED(hr)) return hr;
            if (count > 1) return E_INVALIDARG;
            if (count == 1) return selection->GetItemAt(0, result);
        }
        if (!site) return E_FAIL;
        ComPtr<IServiceProvider> services;
        HRESULT hr = site.As(&services);
        if (FAILED(hr)) return hr;
        ComPtr<IFolderView> view;
        hr = services->QueryService(SID_SFolderView, IID_PPV_ARGS(&view));
        if (FAILED(hr)) return hr;
        return view->GetFolder(IID_PPV_ARGS(result));
    }
public:
    Command() { ++objects; }
    ~Command() { --objects; }
    IFACEMETHODIMP QueryInterface(REFIID iid, void** result) override {
        if (!result) return E_POINTER;
        *result = nullptr;
        if (iid == IID_IUnknown || iid == __uuidof(IExplorerCommand))
            *result = static_cast<IExplorerCommand*>(this);
        else if (iid == IID_IObjectWithSite) *result = static_cast<IObjectWithSite*>(this);
        else return E_NOINTERFACE;
        AddRef();
        return S_OK;
    }
    IFACEMETHODIMP_(ULONG) AddRef() override { return ++references; }
    IFACEMETHODIMP_(ULONG) Release() override {
        const ULONG count = --references;
        if (!count) delete this;
        return count;
    }
    IFACEMETHODIMP GetTitle(IShellItemArray*, LPWSTR* title) override {
        return title ? SHStrDupW(L"Edolterm\uc5d0\uc11c \uc5f4\uae30", title) : E_POINTER;
    }
    IFACEMETHODIMP GetIcon(IShellItemArray*, LPWSTR* icon) override {
        if (!icon) return E_POINTER;
        *icon = nullptr;
        try { return SHStrDupW((executable().wstring() + L",0").c_str(), icon); }
        catch (...) { return E_FAIL; }
    }
    IFACEMETHODIMP GetToolTip(IShellItemArray*, LPWSTR* tip) override {
        if (!tip) return E_POINTER;
        *tip = nullptr;
        return E_NOTIMPL;
    }
    IFACEMETHODIMP GetCanonicalName(GUID* id) override {
        if (!id) return E_POINTER;
        *id = commandId;
        return S_OK;
    }
    IFACEMETHODIMP GetState(IShellItemArray* selection, BOOL, EXPCMDSTATE* state) override {
        if (!state) return E_POINTER;
        *state = ECS_HIDDEN;
        ComPtr<IShellItem> item;
        if (FAILED(location(selection, &item))) return S_OK;
        SFGAOF attributes = 0;
        const auto required = SFGAO_FILESYSTEM | SFGAO_FOLDER;
        if (SUCCEEDED(item->GetAttributes(required | SFGAO_STREAM, &attributes)) &&
            (attributes & required) == required && !(attributes & SFGAO_STREAM))
            *state = ECS_ENABLED;
        return S_OK;
    }
    IFACEMETHODIMP Invoke(IShellItemArray* selection, IBindCtx*) override {
        try {
            EXPCMDSTATE state;
            GetState(selection, TRUE, &state);
            if (state != ECS_ENABLED) return E_INVALIDARG;
            ComPtr<IShellItem> item;
            HRESULT hr = location(selection, &item);
            if (FAILED(hr)) return hr;
            PWSTR raw = nullptr;
            hr = item->GetDisplayName(SIGDN_FILESYSPATH, &raw);
            if (FAILED(hr)) return hr;
            std::wstring directory;
            try { directory = raw; } catch (...) { CoTaskMemFree(raw); throw; }
            CoTaskMemFree(raw);
            const auto app = executable().wstring();
            auto arguments = quote(app) + L" --working-directory " + quote(directory);
            PROCESS_INFORMATION process{};
            if (!launch_terminal(app, arguments, directory.c_str(), process))
                return HRESULT_FROM_WIN32(GetLastError());
            CloseHandle(process.hThread);
            CloseHandle(process.hProcess);
            return S_OK;
        } catch (...) { return E_FAIL; }
    }
    IFACEMETHODIMP GetFlags(EXPCMDFLAGS* flags) override {
        if (!flags) return E_POINTER;
        *flags = ECF_DEFAULT;
        return S_OK;
    }
    IFACEMETHODIMP EnumSubCommands(IEnumExplorerCommand** commands) override {
        if (!commands) return E_POINTER;
        *commands = nullptr;
        return E_NOTIMPL;
    }
    IFACEMETHODIMP SetSite(IUnknown* value) override { site = value; return S_OK; }
    IFACEMETHODIMP GetSite(REFIID iid, void** result) override {
        if (!result) return E_POINTER;
        *result = nullptr;
        return site ? site->QueryInterface(iid, result) : E_FAIL;
    }
};

class Factory final : public IClassFactory {
    std::atomic<ULONG> references{1};
public:
    Factory() { ++objects; }
    ~Factory() { --objects; }
    IFACEMETHODIMP QueryInterface(REFIID iid, void** result) override {
        if (!result) return E_POINTER;
        *result = nullptr;
        if (iid != IID_IUnknown && iid != IID_IClassFactory) return E_NOINTERFACE;
        *result = static_cast<IClassFactory*>(this); AddRef(); return S_OK;
    }
    IFACEMETHODIMP_(ULONG) AddRef() override { return ++references; }
    IFACEMETHODIMP_(ULONG) Release() override {
        ULONG count = --references; if (!count) delete this; return count;
    }
    IFACEMETHODIMP CreateInstance(IUnknown* outer, REFIID iid, void** result) override {
        if (!result) return E_POINTER;
        *result = nullptr;
        if (outer) return CLASS_E_NOAGGREGATION;
        auto command = new (std::nothrow) Command;
        if (!command) return E_OUTOFMEMORY;
        HRESULT hr = command->QueryInterface(iid, result);
        command->Release(); return hr;
    }
    IFACEMETHODIMP LockServer(BOOL lock) override {
        if (lock) ++objects; else --objects; return S_OK;
    }
};

extern "C" HRESULT __stdcall DllGetClassObject(REFCLSID clsid, REFIID iid, void** result) {
    if (!result) return E_POINTER;
    *result = nullptr;
    if (clsid != commandId) return CLASS_E_CLASSNOTAVAILABLE;
    auto factory = new (std::nothrow) Factory;
    if (!factory) return E_OUTOFMEMORY;
    HRESULT hr = factory->QueryInterface(iid, result);
    factory->Release(); return hr;
}
extern "C" HRESULT __stdcall DllCanUnloadNow() { return objects == 0 ? S_OK : S_FALSE; }
BOOL WINAPI DllMain(HINSTANCE instance, DWORD reason, LPVOID) {
    if (reason == DLL_PROCESS_ATTACH) { module = instance; DisableThreadLibraryCalls(instance); }
    return TRUE;
}
