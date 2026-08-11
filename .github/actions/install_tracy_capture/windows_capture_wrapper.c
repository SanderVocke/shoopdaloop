#ifndef UNICODE
#define UNICODE
#endif
#ifndef _UNICODE
#define _UNICODE
#endif
#include <windows.h>
#include <stdio.h>
#include <stdlib.h>
#include <wchar.h>

static void print_last_error(const wchar_t *operation) {
    fwprintf(stderr, L"shoop-tracy-capture-wrapper: %ls failed (%lu)\n",
             operation, GetLastError());
}

int wmain(int argc, wchar_t **argv) {
    if (argc == 2 && wcscmp(argv[1], L"--help") == 0) {
        wprintf(L"Usage: shoop-tracy-capture-wrapper.exe "
                L"<tracy-capture.exe> <output.tracy> <stop-request>\n");
        return 0;
    }
    if (argc != 4) {
        fwprintf(stderr,
                 L"Usage: shoop-tracy-capture-wrapper.exe "
                 L"<tracy-capture.exe> <output.tracy> <stop-request>\n");
        return 2;
    }

    const wchar_t *tool = argv[1];
    const wchar_t *output = argv[2];
    const wchar_t *stop_request = argv[3];
    size_t command_len = wcslen(tool) + wcslen(output) + 16;
    wchar_t *command = (wchar_t *)calloc(command_len, sizeof(wchar_t));
    if (!command) {
        fwprintf(stderr, L"shoop-tracy-capture-wrapper: allocation failed\n");
        return 3;
    }
    _snwprintf_s(command, command_len, _TRUNCATE,
                 L"\"%ls\" -o \"%ls\"", tool, output);

    STARTUPINFOW startup;
    PROCESS_INFORMATION process;
    ZeroMemory(&startup, sizeof(startup));
    ZeroMemory(&process, sizeof(process));
    startup.cb = sizeof(startup);
    startup.dwFlags = STARTF_USESTDHANDLES;
    startup.hStdInput = GetStdHandle(STD_INPUT_HANDLE);
    startup.hStdOutput = GetStdHandle(STD_OUTPUT_HANDLE);
    startup.hStdError = GetStdHandle(STD_ERROR_HANDLE);

    /* The Rust parent gives this wrapper a private console. tracy-capture
       inherits that console and these redirected log handles. */
    SetConsoleCtrlHandler(NULL, FALSE);
    if (!CreateProcessW(tool, command, NULL, NULL, TRUE, 0, NULL, NULL,
                        &startup, &process)) {
        print_last_error(L"CreateProcessW");
        free(command);
        return 4;
    }
    free(command);
    CloseHandle(process.hThread);

    DWORD exit_code = 1;
    BOOL stop_sent = FALSE;
    for (;;) {
        DWORD wait_result = WaitForSingleObject(process.hProcess, 25);
        if (wait_result == WAIT_OBJECT_0) {
            if (!GetExitCodeProcess(process.hProcess, &exit_code)) {
                print_last_error(L"GetExitCodeProcess");
                exit_code = 5;
            }
            break;
        }
        if (wait_result == WAIT_FAILED) {
            print_last_error(L"WaitForSingleObject");
            TerminateProcess(process.hProcess, 6);
            WaitForSingleObject(process.hProcess, INFINITE);
            exit_code = 6;
            break;
        }

        if (!stop_sent && GetFileAttributesW(stop_request) != INVALID_FILE_ATTRIBUTES) {
            /* CTRL_C_EVENT cannot target one process group. Ignore our own
               broadcast and send it to every process in this private console;
               only this wrapper and tracy-capture are attached. */
            if (!SetConsoleCtrlHandler(NULL, TRUE)) {
                print_last_error(L"SetConsoleCtrlHandler");
                TerminateProcess(process.hProcess, 7);
                WaitForSingleObject(process.hProcess, INFINITE);
                exit_code = 7;
                break;
            }
            if (!GenerateConsoleCtrlEvent(CTRL_C_EVENT, 0)) {
                print_last_error(L"GenerateConsoleCtrlEvent");
                TerminateProcess(process.hProcess, 8);
                WaitForSingleObject(process.hProcess, INFINITE);
                exit_code = 8;
                break;
            }
            stop_sent = TRUE;
        }
    }

    DeleteFileW(stop_request);
    CloseHandle(process.hProcess);
    return (int)exit_code;
}
