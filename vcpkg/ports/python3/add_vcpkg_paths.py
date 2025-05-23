
def add_vcpkg_paths():
    import sys
    if sys.platform == "win32":
        import os
        script_dir = os.path.dirname(os.path.abspath(__file__))
        root = os.path.join(script_dir, '../../../..')
        sys.path.append(os.path.join(root, 'lib', 'site-packages'))
        os.add_dll_directory(os.path.join(root, "bin"))
