import QtQuick 2.15
import QtQuick.Controls 2.15
import SLLooperState 1.0
import SLLooperManager 1.0

ApplicationWindow {
    visible: true
    width: 600
    height: 600
    title: "ShoopDaLoop"

    component LoopProgressIndicator : Row {
        property alias length: pb.length
        property alias pos: pb.pos

        ProgressBar {
            id: pb
            property double length
            property double pos

            value: pos / length
        }

        Text {
            text: pb.pos.toFixed(2)
            color: "#000000"
        }
    }

    Column {
        Repeater {
            model: 4
            id: loops

            LoopProgressIndicator {
                length: looper_state.length
                pos: looper_state.pos

                SLLooperState {
                    id: looper_state
                }
                SLLooperManager {
                    id: looper_mgr
                    sl_looper_index: index
                }
                Connections {
                    target: osc_link
                    function onReceived(msg) { looper_mgr.onOscReceived(msg) }
                }
                Connections {
                    target: looper_mgr
                    function onLengthChanged(len) { looper_state.length = len }
                    function onPosChanged(pos) { looper_state.pos = pos }
                    function onSendOscExpectResponse(msg, ret) { osc_link.send_expect_response(msg, ret) }
                }

                Component.onCompleted: {
                    looper_mgr.setup_sync()
                }
            }
        }
    }
}