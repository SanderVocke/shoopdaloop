import QtQuick 6.6
import ShoopDaLoop.PythonLogger

QtObject {
    id: root
    property var descriptor
    property string schema
    property bool enabled: true
    property string object_description: 'unknown'

    readonly property PythonLogger logger : PythonLogger { name: "Frontend.Qml.SchemaCheck" }
    Component.onCompleted: maybe_check()
    onEnabledChanged: maybe_check()
    
    function maybe_check() {
        if (!enabled) { return; }

        try {
            schema_validator.validate_schema(descriptor, schema)
        } catch(err) {
            root.logger.warning(`Failed session schema validation for object of type ${schema}:\n` +
                        `\nobject (description "${object_description}"):\n  ${JSON.stringify(descriptor, 0, 2)}\nerror:\n  ${err.message}`)
        }
    }
}