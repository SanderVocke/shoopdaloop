import QtQuick 6.6
import ShoopDaLoop.PythonLogger

// Generic settings with tools for reading, writing upgrading settings data.
QtObject {
    id: root
    
    property string name: "Unknown"
    property string schema_name: 'unknown'

    // Note: don't set directly! Use from_dict instead.
    property var contents: []
    property var current_version: 1

    readonly property PythonLogger logger: PythonLogger { name: `Frontend.Qml.Settings.${name}` }

    function validate() {
        let dict = to_dict()
        let schema = `${schema_name}.${current_version}`
        try {
            schema_validator.validate_schema(dict, schema)
        } catch (e) {
            root.logger.error(() => (`Failed to validate ${name} settings against ${schema}: ${e.message}. Config: ${JSON.stringify(dict, 0, 2)}`))
            return false
        }
        return true
    }

    onContentsChanged: validate()
    Component.onCompleted: validate()

    function from_dict(dict) {
        while(dict['schema'] != `${schema_name}.${current_version}`) {
            dict = upgrade(dict)
        }
        contents = dict['configuration']
    }
    function to_dict() {
        return {
            'schema': `${schema_name}.1`,
            'configuration': contents
        }
    }

    function upgrade(dict) {
        let version_regex = /^/ + schema_name + /\.(\d+)$/
        let match = version_regex.exec(dict['schema'])

        if (match && match[1] == 1) {
            return dict
        }
        throw new Error(`Unrecognized schema for ${name} settings: ${dict['schema']}`)
    }
}