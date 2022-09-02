import QtQuick 2.15
import QtQuick.Controls 2.15
import LoopState 1.0

ApplicationWindow {
    visible: true
    width: 1000
    height: 800
    title: "ShoopDaLoop"

    component LoopProgressIndicator : ProgressBar {
        property double length
        property double pos

        value: pos / length
    }

    Column {
        Repeater {
            model: 4
            id: loops

            Item {

                LoopState {
                    sl_loop_index : index
                    id : self
                    Connections {
                        target: osc_link
                        function onReceivedLoopParam(idx, ctl, val) {
                            self.onLoopParameterChanged()
                        }
                    }
                }
            }
        }
    }
}