#!usr/bin/env python3

import sys
import os
import shutil

def replace_symlink_with_target(filename):
    if os.path.islink(filename):
        target_path = os.path.realpath(filename)
        os.unlink(filename)
        shutil.copy(target_path, filename)
        print(f"Replaced symlink {filename} with its target")

replace_symlink_with_target(sys.argv[1])