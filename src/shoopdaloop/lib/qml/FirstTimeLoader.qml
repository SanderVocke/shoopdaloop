import QtQuick 6.6

Loader {
    property bool activate : false
    active: false

    Component.onCompleted: if (first_time_active) { active = true; }
    onFirst_time_activeChanged: if (first_time_active) { active = true; }
}