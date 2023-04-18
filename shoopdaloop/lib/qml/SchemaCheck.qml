import QtQuick 2.15

QtObject {
    property var descriptor
    property string schema

    Component.onCompleted: {
        try {
            schema_validator.validate_schema(descriptor, schema)
        } catch(err) {
            console.log("Failed session schema validation for object of type ", schema, ":\n",
                        "\nobject:\n", descriptor, "\nerror:\n", err.message)
        }
    }
}