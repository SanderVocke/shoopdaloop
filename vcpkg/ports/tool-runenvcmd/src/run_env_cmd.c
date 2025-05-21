#include <windows.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

int main(int argc, char *argv[]) {
    char *base_cmd = getenv("CMD_TO_RUN");
    if (base_cmd == NULL) {
        fprintf(stderr, "Environment variable CMD_TO_RUN is not set.\n");
        return 1;
    }

    // Build the full command string
    // Start with the base command
    char command_line[1024] = {0};
    strcat(command_line, base_cmd);

    // Add space and each argument
    for (int i = 1; i < argc; i++) {
        strcat(command_line, " ");
        strcat(command_line, argv[i]);
    }

    // Wrap the command in cmd.exe call
    char full_cmd[1100];
    snprintf(full_cmd, sizeof(full_cmd), "cmd.exe /C \"%s\"", command_line);

    // Set up process information
    STARTUPINFO si = {0};
    PROCESS_INFORMATION pi = {0};
    si.cb = sizeof(si);

    // Create the process
    if (!CreateProcess(
            NULL,
            full_cmd,
            NULL, NULL, FALSE, 0,
            NULL, NULL,
            &si, &pi)) {
        fprintf(stderr, "CreateProcess failed (%lu).\n", GetLastError());
        return 1;
    }

    // Wait for the process to finish
    WaitForSingleObject(pi.hProcess, INFINITE);

    // Clean up
    CloseHandle(pi.hProcess);
    CloseHandle(pi.hThread);

    return 0;
}
