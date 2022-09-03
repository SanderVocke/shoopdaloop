import QtQuick 2.15
import QtQuick.Controls 2.15
import SLLooperState 1.0
import SLLooperManager 1.0

ApplicationWindow {
    visible: true
    width: 600
    height: 600
    title: "ShoopDaLoop"

    component LoopProgressIndicator : Item {
        property alias length: pb.length
        property alias pos: pb.pos
        property bool active

        width: pb.width
        height: pb.height

        ProgressBar {
            width: 80
            height: 25
            id: pb
            property double length
            property double pos

            value: active ? pos / length : 0.0
        }
        Text {
            text: active ? pb.pos.toFixed(2) : "(inactive)"
            color: "#000000"
        }  
    }

    component LoopWidget : Item {
        property int loop_idx
        property var osc_link_obj

        // State and OSC management
        SLLooperState {
            id: looper_state
        }
        SLLooperManager {
            id: looper_mgr
            sl_looper_index: loop_idx
        }
        Connections {
            target: osc_link_obj
            function onReceived(msg) { looper_mgr.onOscReceived(msg) }
        }
        Connections {
            target: looper_mgr
            function onLengthChanged(len) { looper_state.length = len }
            function onPosChanged(pos) { looper_state.pos = pos }
            function onActiveChanged(active) { looper_state.active = active }
            function onSendOscExpectResponse(msg, ret) { osc_link_obj.send_expect_response(msg, ret) }
            function onSendOsc(msg) { osc_link_obj.send(msg) }
        }

        // Initialization
        Component.onCompleted: {
            looper_mgr.setup_sync()
        }

            // UI
        Row {
            Button {
                width: 50
                height: progress.height
                text: "Trig"
                onClicked: looper_mgr.doTrigger()
            }
            LoopProgressIndicator {
                id: progress
                length: looper_state.length
                pos: looper_state.pos
                active: looper_state.active
            }
        }
    }

    Column {
        Repeater {
            model: 8
            id: loops

            Item {
                // TODO
                height:30
                width:100
                LoopWidget {
                    loop_idx: index
                    osc_link_obj: osc_link
                }
            }
        }
    }
}