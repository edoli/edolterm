# HANDLE marshalling is RPC, not Automation. Build our own proxy so the endpoint
# can marshal outside the Microsoft package's private COM registration context.
get_filename_component(handoff_sdk "[HKEY_LOCAL_MACHINE\\SOFTWARE\\Microsoft\\Windows Kits\\Installed Roots;KitsRoot10]" ABSOLUTE)
find_program(HANDOFF_MIDL midl.exe HINTS "${handoff_sdk}/bin/${CMAKE_VS_WINDOWS_TARGET_PLATFORM_VERSION}/x64" REQUIRED)
set(handoff_source "${CMAKE_CURRENT_LIST_DIR}")
set(handoff_generated "${CMAKE_CURRENT_BINARY_DIR}/handoff-proxy")
file(MAKE_DIRECTORY "${handoff_generated}")
add_custom_command(
    OUTPUT "${handoff_generated}/TerminalHandoff_h.h" "${handoff_generated}/TerminalHandoff_i.c"
           "${handoff_generated}/TerminalHandoff_p.c" "${handoff_generated}/dlldata.c"
    COMMAND "${HANDOFF_MIDL}" /nologo /env x64 /target NT100 /robust /out "${handoff_generated}"
            /h TerminalHandoff_h.h /iid TerminalHandoff_i.c /proxy TerminalHandoff_p.c /dlldata dlldata.c
            /I "${handoff_sdk}/Include/${CMAKE_VS_WINDOWS_TARGET_PLATFORM_VERSION}/um"
            /I "${handoff_sdk}/Include/${CMAKE_VS_WINDOWS_TARGET_PLATFORM_VERSION}/shared"
            "${handoff_source}/TerminalHandoff.idl"
    DEPENDS "${handoff_source}/TerminalHandoff.idl" VERBATIM)
add_library(EdoltermHandoffProxy SHARED "${handoff_source}/HandoffProxy.c" "${handoff_source}/HandoffProxy.def"
    "${handoff_generated}/TerminalHandoff_i.c" "${handoff_generated}/TerminalHandoff_p.c" "${handoff_generated}/dlldata.c")
# dlldata is included by our wrapper to select an Edolterm-owned proxy CLSID.
set_source_files_properties("${handoff_generated}/dlldata.c" PROPERTIES HEADER_FILE_ONLY TRUE)
target_include_directories(EdoltermHandoffProxy PRIVATE "${handoff_generated}")
target_compile_definitions(EdoltermHandoffProxy PRIVATE _WIN32_WINNT=0x0A00)
target_link_libraries(EdoltermHandoffProxy PRIVATE rpcrt4 oleaut32 uuid)
set_target_properties(EdoltermHandoffProxy PROPERTIES OUTPUT_NAME "Edolterm.HandoffProxy"
    MSVC_RUNTIME_LIBRARY "MultiThreaded$<$<CONFIG:Debug>:Debug>")
add_library(EdoltermHandoffInterfaces INTERFACE)
target_include_directories(EdoltermHandoffInterfaces INTERFACE "${handoff_generated}")
add_dependencies(EdoltermHandoffInterfaces EdoltermHandoffProxy)
