#!usr/bin/env python3
import sys
import tempfile
import subprocess
import locale
import os

dumpsym=sys.argv[1]
binary=sys.argv[2]
symbolsdir=sys.argv[3]

def main():
    print("Creating breakpad symbols in {} for {}".format(symbolsdir, binary))
    
    procresult = subprocess.run([dumpsym, binary], capture_output=True, encoding=locale.getencoding())
    print(procresult.stderr, file=sys.stderr)
    if procresult.returncode != 0:
        return procresult.returncode
    
    symbols = procresult.stdout
    first_line = symbols.split('\n')[0].strip()
    words = first_line.split(' ')

    binary_name = words[4]
    binary_id = words[3]

    binary_bare_name = binary_name
    if binary_bare_name[-4:] == '.pdb':
        binary_bare_name = binary_bare_name[:-4]
    
    finaldir = os.path.join(symbolsdir, binary_name, binary_id)
    finalfile = os.path.join(finaldir, binary_bare_name + '.sym')

    print("Got symbols for {} ({}). Writing to: {}".format(binary_name, binary_id, finalfile))
    os.makedirs(finaldir, exist_ok=True)
    with open(finalfile, 'w') as f:
        f.write(symbols)

if __name__ == "__main__":
    main()