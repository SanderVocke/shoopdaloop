function is_object(a) {
    return a != null && typeof a === 'object';
}

function is_array(a) {
    // In PySide up to 6.7.0, doing Array.isArray() would work even
    // with arrays returned from Python, but 6.7.0 onwards seems to
    // break this.
    return is_object(a) && a.length !== undefined && (a.length == 0 || a[0] !== undefined)
}