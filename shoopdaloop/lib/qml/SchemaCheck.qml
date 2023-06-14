import QtQuick 6.3

QtObject {
    property var descriptor
    property string schema
    property bool enabled: true

    Component.onCompleted: maybe_check()
    onEnabledChanged: maybe_check()
    
    function maybe_check() {
        if (!enabled) { return; }

        try {
            schema_validator.validate_schema(descriptor, schema)
        } catch(err) {
            console.log("Failed session schema validation for object of type ", schema, ":\n",
                        "\nobject:\n", JSON.stringify(descriptor, 0, 2), "\nerror:\n", err.message)
        }
    }
}