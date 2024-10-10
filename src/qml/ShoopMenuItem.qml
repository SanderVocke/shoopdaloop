import QtQuick 6.6
import QtQuick.Controls 6.6
import QtQuick.Controls.Material 6.6

MenuItem {
    property bool shown: true

    visible: shown
    height: shown ? 30 : 0
}