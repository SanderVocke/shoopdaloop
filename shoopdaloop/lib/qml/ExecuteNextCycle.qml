import QtQuick 6.3
import QtQuick.Controls 6.3

Timer {
    id: root
    interval: 1
    signal execute()
    onTriggered: execute()

    function trigger() { restart() }
}