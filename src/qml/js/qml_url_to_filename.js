function qml_url_to_filename(url) {
    var result = url.replace('file://', '')
    if (Qt.platform.os === "windows") {
        result = result.replace(/^\//, '')
        result = result.replace('/', '\\')
    }
    return result
}