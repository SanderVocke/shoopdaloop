#include <stdlib.h>
#include <string.h>

char *basename(char *path) {
    static char fname[_MAX_FNAME] = {0};

    strncpy_s(fname, sizeof(fname), path, strlen(path));
    fname[sizeof(fname) - 1] = 0;

    _splitpath_s(path, NULL, 0, NULL, 0, fname, sizeof(fname), NULL, 0);

    return fname;
}

char *dirname(char *path) {
    static char dname[_MAX_DIR] = {0};

    strncpy_s(dname, sizeof(dname), path, strlen(path));
    dname[sizeof(dname) - 1] = 0;

    _splitpath_s(path, NULL, 0, dname, sizeof(dname), NULL, 0, NULL, 0);

    return dname;
}
