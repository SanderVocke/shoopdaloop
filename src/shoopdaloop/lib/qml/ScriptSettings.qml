import QtQuick 6.3

Settings {
    name: 'ScriptSettings'
    schema_name: 'script_settings'
    current_version: 1

    contents: default_contents()

    property list<var> known_scripts: contents['known_scripts']

    function default_contents() { return ({
            'known_scripts': []
        })
    }
}