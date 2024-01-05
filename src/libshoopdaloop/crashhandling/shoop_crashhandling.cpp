// From Breakpad/src/client/<os>
#include "shoop_crashhandling.h"
#include <handler/exception_handler.h>
#include <iostream>
#include <memory>
#include <stdexcept>
#include <locale>
#include <codecvt>

std::unique_ptr<google_breakpad::ExceptionHandler> g_exception_handler;
CrashedCallback g_crashed_callback = nullptr;

#if __APPLE__
static bool dumpCallback(const char* dump_dir, const char* minidump_id, void* context, bool succeeded) {
    std::cout << "ShoopDaLoop crashed. Breakpad crash minidump saved @ " << dump_dir << "/" << minidump_id << ".dmp." << std::endl;
    std::string _dd(dump_dir), _did(minidump_id);
    std::string filename = _dd + "/" + _did + ".dmp";
    if (g_crashed_callback) {
        g_crashed_callback(filename.c_str());
    }
    return succeeded;
}
#elif defined(_WIN32)
static bool dumpCallback(const wchar_t* dump_dir, const wchar_t* minidump_id, void* context, EXCEPTION_POINTERS* exinfo, MDRawAssertionInfo *assertion, bool succeeded) {
    std::wcout << "ShoopDaLoop crashed. Breakpad crash minidump saved @ " << dump_dir << "/" << minidump_id << ".dmp." << std::endl;
    std::wstring _dd(dump_dir), _did(minidump_id);
    std::wstring wfilename = _dd + L"/" + _did + L".dmp";
    std::string filename = std::wstring_convert<std::codecvt_utf8<wchar_t>>().to_bytes(wfilename);
    if (g_crashed_callback) {
        g_crashed_callback(filename.c_str());
    }
    return succeeded;
}
#else
static bool dumpCallback(const google_breakpad::MinidumpDescriptor& descriptor,
void* context, bool succeeded) {
    std::cout << "ShoopDaLoop crashed. Breakpad crash minidump saved @ " << descriptor.path() << "." << std::endl;
    if (g_crashed_callback) {
        g_crashed_callback(descriptor.path());
    }
    return succeeded;
}
#endif

extern "C" {

void shoop_init_crashhandling_with_cb(const char* dump_dir, CrashedCallback cb) {
    g_crashed_callback = cb;
#if __APPLE__
    g_exception_handler = std::make_unique<google_breakpad::ExceptionHandler>(dump_dir, nullptr, dumpCallback, nullptr, true, nullptr);
#elif defined(_WIN32)
    std::wstring wide(dump_dir, dump_dir + strlen(dump_dir));
    g_exception_handler = std::make_unique<google_breakpad::ExceptionHandler>(wide, nullptr, dumpCallback, nullptr, google_breakpad::ExceptionHandler::HANDLER_ALL);
#else
    google_breakpad::MinidumpDescriptor descriptor(dump_dir);
    g_exception_handler = std::make_unique<google_breakpad::ExceptionHandler>(descriptor, nullptr, dumpCallback, nullptr, true, -1);
#endif
}

void shoop_init_crashhandling(const char* dump_dir) {
    shoop_init_crashhandling_with_cb(dump_dir, (CrashedCallback)nullptr);
}

void shoop_test_crash_segfault() {
    volatile int* a = (int*)(NULL); *a = 1;
}

void shoop_test_crash_exception() {
    throw std::runtime_error("ShoopDaLoop test crash (Exception)");
}

void shoop_test_crash_abort() {
    abort();
}

}