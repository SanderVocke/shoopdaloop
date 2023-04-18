import QtQuick 2.15
import QtQuick.Controls 2.15

Timer {
    id: root
    interval: 1
    signal execute()
    onTriggered: execute()

    function trigger() { restart() }
}