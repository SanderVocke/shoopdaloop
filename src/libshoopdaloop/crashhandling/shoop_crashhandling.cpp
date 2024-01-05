// From Breakpad/src/client/<os>
#include "shoop_crashhandling.h"
#include <handler/exception_handler.h>
#include <iostream>
#include <memory>
#include <stdexcept>
#include <locale>
#include <codecvt>

#if __APPLE__
static bool dumpCallback(const char* dump_dir, const char* minidump_id, void* context, bool succeeded) {
    std::cout << "ShoopDaLoop crashed. Breakpad crash dump saved @ " << dump_dir << "/" << minidump_id << ".dmp." << std::endl;
    return succeeded;
}
#elif defined(_WIN32)
static bool dumpCallback(const wchar_t* dump_dir, const wchar_t* minidump_id, void* context, EXCEPTION_POINTERS* exinfo, MDRawAssertionInfo *assertion, bool succeeded) {
    std::wcout << "ShoopDaLoop crashed. Breakpad crash dump saved @ " << dump_dir << "/" << minidump_id << ".dmp." << std::endl;
    return succeeded;
}
#else
static bool dumpCallback(const google_breakpad::MinidumpDescriptor& descriptor,
void* context, bool succeeded) {
    std::cout << "ShoopDaLoop crashed. Breakpad crash dump saved @ " << descriptor.path() << "." << std::endl;
    return succeeded;
}
#endif

std::unique_ptr<google_breakpad::ExceptionHandler> g_exception_handler;

extern "C" {

void shoop_init_crashhandling(const char* dump_dir) {
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