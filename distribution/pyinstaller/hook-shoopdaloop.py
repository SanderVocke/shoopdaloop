from PyInstaller.utils.hooks import collect_data_files, collect_dynamic_libs
import os

datas = collect_data_files(
    'shoopdaloop', 
    include_py_files=False,
    includes=[
        '**/*',
        '*'
    ],
    excludes=[
        '**/*.cpp',
        '**/example',
        '**/*.py',
        '**/*.pyc'        
    ])

# Manually add a few files
version_txt_path = [p for p in datas if os.path.basename(p[0]) == 'version.txt'][0][0]
basepath = os.path.dirname(version_txt_path)

datas += [
    (basepath + '/libshoopdaloop_bindings.py', 'shoopdaloop'),
    (basepath + '/shoop_crashhandling.py', 'shoopdaloop'),
    (basepath + '/shoop_accelerate.py', 'shoopdaloop')
]

binaries = collect_dynamic_libs(
    'shoopdaloop',
    search_patterns=['*.dll', '*.dylib', '*.so', '*.so*']
)