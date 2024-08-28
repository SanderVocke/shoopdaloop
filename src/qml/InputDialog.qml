import QtQuick 6.6
import QtQuick.Controls 6.6
import QtQuick.Controls.Material 6.6

Dialog {
    id: root
    standardButtons: Dialog.Ok | Dialog.Cancel
    parent: Overlay.overlay
    x: (parent.width - width) / 2
    y: (parent.height - height) / 2
    modal: true

    title: "Title"
    width: 200
    height: 200

    property alias validator: input_field.validator
    property alias default_value: input_field.text

    signal acceptedInput(string input)

    ShoopTextField {
        id: input_field
    }

    onAccepted: acceptedInput(input_field.text)
}