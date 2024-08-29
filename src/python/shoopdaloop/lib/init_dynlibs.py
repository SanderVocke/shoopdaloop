import platform

def init_dynlibs():
    if platform.system() == "Windows":
        # We may have indirect DLL dependencies to DLLs that are packaged
        # with ShoopDaLoop. Add the package directory to our DLL search
        # path.
        import shoop_app_info
        import win32api

        d = shoop_app_info.dynlib_dir
        win32api.SetDllDirectory(d)