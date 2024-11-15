exe="$1"
handled=""
exe_dir=$(dirname "$exe")
exe_dir_escaped=$(dirname "$exe" | sed 's/\//\\\//g')

search_dirs=$(otool -l "$exe" | grep -A2 LC_RPATH | grep -E " path [/@]" | awk '{print $2}' | sed "s/@loader_path/$exe_dir_escaped/g")
for d in $(echo $DYLD_LIBRARY_PATH | tr ':' '\n'); do search_dirs=$(printf "$search_dirs\n$d"); done
for d in $(echo $DYLD_FALLBACK_LIBRARY_PATH | tr ':' '\n'); do search_dirs=$(printf "$search_dirs\n$d"); done
for d in $(echo $DYLD_FRAMEWORK_PATH | tr ':' '\n'); do search_dirs=$(printf "$search_dirs\n$d"); done
echo "Search dirs: $search_dirs" >&2

function resolve_rpath() {
    if [ -f "$1" ]; then echo $1; return
    elif [[ $1 == *"@rpath"* ]]; then
        for dir in $search_dirs; do
            trypath="$dir/$(echo $1 | sed 's/@rpath//g')"
            if [ -f "$trypath" ]; then
                echo "$trypath"
                return
            fi
        done
        echo "Could not find '$1' in search paths, skipping." >&2
    fi
}

function recurse_deps() {
    for f in $(otool -L "$1" | tail -n +1 | awk 'NR>1 {print $1}' || true); do
        resolved=$(resolve_rpath "$f" "$2")
        resolved_filename=$(basename "$resolved")
        filtered=$(echo $handled | grep "$resolved_filename")
        if [ ! -z "$resolved" ] && [ -z "$filtered" ]; then
            echo "${2}$resolved"
            handled=$(printf "$handled\n$resolved_filename")
            recurse_deps "$resolved" "$2  "
        fi 
    done
}
recurse_deps "$exe" ""