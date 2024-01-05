from shoopdaloop.lib.directories import installed_dir
import platform
import subprocess
import sys

def format_crash_dump(dmp_filename):
    print ('Formatting crash dump: {}'.format(dmp_filename))

    symbols_dir = installed_dir + '/crash_handling/breakpad_symbols'
    stackwalk = installed_dir + '/crash_handling/minidump_stackwalk'

    if platform.system() == 'Windows':
        stackwalk += '.exe'
    
    procresult = subprocess.run([stackwalk, dmp_filename, symbols_dir], capture_output=True, encoding='utf8')
    return {
        'exitcode': procresult.returncode,
        'stderr': procresult.stderr,
        'stdout': procresult.stdout
    }




    