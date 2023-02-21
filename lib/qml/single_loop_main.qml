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
        id: backend
        
        client_name_hint: "single_loop"
        update_interval_ms: 30

        AudioPortPair {
            id: audio_ports
            input_name_hint: 'audio_in'
            output_name_hint: 'audio_out'
        }
        MidiPortPair {
            id: midi_ports
            input_name_hint: 'midi_in'
            output_name_hint: 'midi_out'
        }

        Loop {
            id: loop
            LoopAudioChannel {
                loop: loop
                ports: audio_ports.ports
            }
            LoopMidiChannel {
            }
        }
    }

    LoopWidget {
        is_in_hovered_scene: false
        is_in_selected_scene: false
        name: "Loop"
        backend_loop: loop
        master_loop: null
        targeted_loop: null
    }
}
