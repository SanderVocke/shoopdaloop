import QtQuick 2.15
import QtQuick.Controls 2.15

Item {
    function tick() {
        if (playing && cycle < total_cycles) { cycle++ }
    }
    function play() {
        playing = true
    }
    function stop() {
        playing = false
    }
    function set_cycle(cycle) {cycle = 0}

    property bool playing : false
    property int cycle : 0
    property int total_cycles: 0
}