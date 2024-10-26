handled=""
exe_dir=$(dirname "$1")
exe_dir_escaped=$(dirname "$1" | sed 's/\//\\\//g')
search_dirs=$(otool -l "$1" | grep -A2 LC_RPATH | grep -E " path [/@]" | awk '
NR>1 {print $2}' | sed "s/@loader_path/$exe_dir_escaped/g")

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
recurse_deps "$1" ""