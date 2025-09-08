function is_object(a) {
    return a != null && typeof a === 'object';
}

function is_array(a) {
    return is_object(a) && a.length !== undefined && (a.length == 0 || a[0] !== undefined)
}