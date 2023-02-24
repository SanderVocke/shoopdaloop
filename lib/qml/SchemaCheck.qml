import QtQuick 2.15

QtObject {
    property var descriptor
    property string schema

    Component.onCompleted: {
        console.log("checking ", schema, descriptor)
        schema_validator.validate_schema(descriptor, schema)
    }
}