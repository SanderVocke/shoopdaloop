import sys

dumpsym=sys.argv[1]
executable=sys.argv[2]
symbolsdir=sys.argv[3]

def main():
    # TODO
    # head -n1 breakpad_symbols/shoopdaloop.sym | awk '{print $4}' will print e.g. FEF24ADF244FCAAE903DF3D99C1F73E10
    # head -n1 breakpad_symbols/shoopdaloop.sym | awk '{print $5}' will print e.g. libshoopdaloop.so
    # should put it at: symbolsdir/libshoopdaloop.so/FEF24ADF244FCAAE903DF3D99C1F73E10/libshoopdaloop.so.sym
