import QtQuick 6.6

Item {
    property real linear: 0.0
    property real dB_threshold: -60.0
    property real dB: dB_threshold
    property real dB_dead_zone: 0.1
    
    function update_dB() {
        var new_dB = linear <= 0.0 ?
            dB_threshold :
            20.0 * Math.log(linear) / Math.log(10)
        
        if (Math.abs(new_dB - dB) >= dB_dead_zone) {
            dB = new_dB
        }
    }

    function update_linear() {
        linear = dB <= dB_threshold ? 0.0 : 10 ** (dB/20.0)
    }

    onDBChanged: update_linear()
    onLinearChanged: update_dB()
    onDB_thresholdChanged: { dB = Math.max(dB_threshold, dB); update_linear() }
}