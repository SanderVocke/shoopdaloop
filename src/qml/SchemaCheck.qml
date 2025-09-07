import QtQuick 6.6
import ShoopDaLoop.Rust

QtObject {
    id: root
    property var descriptor
    property string schema
    property bool enabled: true
    property string object_description: '(unknown)'

    readonly property ShoopRustLogger logger : ShoopRustLogger { name: "Frontend.Qml.SchemaCheck" }
    Component.onCompleted: maybe_check()
    onEnabledChanged: maybe_check()
    
    function maybe_check() {
        if (!enabled) { return; }
        ShoopRustSchemaValidator.validate_schema(descriptor, object_description, schema, true)
    }
}