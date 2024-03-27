import QtQuick 6.6
import QtQuick.Controls 6.6

Timer {
    id: root
    interval: 1
    signal execute()
    onTriggered: execute()

    function trigger() { restart() }
}