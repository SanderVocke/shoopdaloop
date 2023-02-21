import QtQuick 2.15
import QtQuick.Controls 2.15
import QtQuick.Controls.Material 2.15
import QtQuick.Dialogs

ApplicationWindow {
    visible: true
    width: 1200
    height: 600
    maximumWidth: width
    maximumHeight: height
    minimumWidth: width
    minimumHeight: height
    title: "ShoopDaLoop Single Loop"
    id: appWindow

    Material.theme: Material.Dark

    // Ensure other controls lose focus when clicked outside
    MouseArea {
        id: background_focus
        anchors.fill: parent
        acceptedButtons: Qt.AllButtons
        onClicked: () => { forceActiveFocus() }
    }

    Backend {
        client_name_hint: "single_loop"
        update_interval_ms: 30

        Loop {
            id: loop

            LoopAudioChannel {
            }
            LoopMidiChannel {
            }
        }

        AudioPortPair {
            input_name_hint: 'audio_in'
            output_name_hint: 'audio_out'
        }
    }

    LoopWidget {
        is_in_hovered_scene: false
        is_in_selected_scene: false
        name: "Loop"
        backend_loop: loop
        master_loop: loop
        targeted_loop: null
    }
}
