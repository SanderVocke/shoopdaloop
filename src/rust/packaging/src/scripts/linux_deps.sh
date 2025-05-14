EXE=$(mktemp);
cp $1 $EXE;
INC_DIR=$2
for f in $(find "$INC_DIR" -type f -name "*.so*"); do patchelf --add-needed $(basename $f) $EXE; done;
RAW=$(LD_LIBRARY_PATH=$(find "$INC_DIR" -type d | xargs printf "%s:" | sed 's/,$/\n/'):$LD_LIBRARY_PATH lddtree $EXE)
printf "$RAW\n" | grep -E "^[ ]" | grep -v "not found" | grep "=>" | grep -E -v "=>.*=>" | sed -r 's/([ ]*).*=>[ ]*([^ ]*).*/\1\2/g';
printf "$RAW\n" | grep "not found" >&2;
rm $EXE;