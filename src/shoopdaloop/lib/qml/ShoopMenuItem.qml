import QtQuick 6.3
import QtQuick.Controls 6.3
import QtQuick.Controls.Material 6.3

MenuItem {
    property bool shown: true

    visible: shown
    height: shown ? 30 : 0
}