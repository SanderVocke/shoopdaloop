from PyInstaller.utils.hooks import collect_data_files, collect_dynamic_libs

datas = collect_data_files("lupa", True)
binaries = collect_dynamic_libs("lupa", search_patterns=["*.dll", "*.dylib", "*.so", "*.so*", "*.pyd"])