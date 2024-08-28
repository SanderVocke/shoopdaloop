import QtQuick 6.6

Loader {
    property bool activate : false
    active: false

    Component.onCompleted: if (activate) { active = true; }
    onActivateChanged: if (activate) { active = true; }
}