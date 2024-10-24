handled=""
exe_dir=$(dirname "$1")
exe_dir_escaped=$(dirname "$1" | sed 's/\//\\\//g')
search_dirs=$(otool -l "$1" | grep -A2 LC_RPATH | grep -E " path [/@]" | awk '
NR>1 {print $2}' | sed "s/@loader_path/$exe_dir_escaped/g")

function direct_deps() {
    if [ -f "$1" ]; then
        path=$(realpath "$1")
        if [ -z "$(echo $handled | grep $path)" ]; then
            otool -L "$path" | tail -n +1 | awk 'NR>1 {print $1}' || true
            handled=$(printf $handled"\n"$path)
        fi
    elif [[ $1 == *"@rpath"* ]]; then
        for dir in $search_dirs; do
            direct_deps "$dir/$(echo $1 | sed 's/@rpath//g')"
        done
    fi
}
function recurse_deps() {
    if [[ $3 -ge $4 ]]; then return 0; fi
    for dep in $(direct_deps "$1"); do
        if [ ! -z "$dep" ]; then
            echo "$2$dep"
            recurse_deps "$dep" "$2  " $(( $3 + 1 )) $4
        fi
    done
}
recurse_deps "$1" "" 0 5