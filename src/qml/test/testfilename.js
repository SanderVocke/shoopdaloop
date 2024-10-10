function test_filename() {
    var str = (new Error()).stack;
    const match = str.match(/.*\/(.*\.qml)/)
    return match[1]
}
