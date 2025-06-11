def add_vcpkg_paths():
    import sys
    if sys.platform == "win32":
        import os
        script_dir = os.path.dirname(os.path.abspath(__file__))
        root = os.path.join(script_dir, '../../../..')
        maybe_sitepackages = os.path.join(root, 'lib', 'site-packages')
        maybe_bin = os.path.join(root, 'bin')
        maybe_lib = os.path.join(root, 'lib')
        if os.path.exists(maybe_sitepackages):
            sys.path.append(maybe_sitepackages)
        if os.path.exists(maybe_bin):
            os.add_dll_directory(os.path.join(root, "bin"))