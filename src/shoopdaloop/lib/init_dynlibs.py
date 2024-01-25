import platform

def init_dynlibs():
    if platform.system() == "Windows":
        # We may have indirect DLL dependencies to DLLs that are packaged
        # with ShoopDaLoop. Add the package directory to our DLL search
        # path.
        import shoopdaloop.lib.directories as directories
        import win32api

        d = directories.installation_dir()
        print(f"SET DLL DIR {d}")
        win32api.SetDllDirectory(d)