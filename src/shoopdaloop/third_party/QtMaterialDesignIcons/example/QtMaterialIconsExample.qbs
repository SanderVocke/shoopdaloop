import qbs

Project {
    minimumQbsVersion: "1.7.1"

    CppApplication {
        Depends { name: "Qt"; submodules: ["core", "widgets", "xml", "svg", "qml" ] }

        cpp.includePaths: [ "../sources/" ]
        cpp.cxxLanguageVersion: "c++11"
        consoleApplication: true
        files: [
            "*.cpp",
            "*.h",
            "../qml/*.qml",
            "../qml/*.js",
            "../resources/*.qrc",
            "../sources/*.h",
            "../sources/*.cpp",
        ]
    }
}
