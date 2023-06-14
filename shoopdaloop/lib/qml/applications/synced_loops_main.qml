import QtQuick 6.3
import QtQuick.Controls 6.3
import QtQuick.Controls.Material 6.3
import QtQuick.Dialogs

import ".."

ApplicationWindow {
    visible: true
    width: 1200
    height: 600
    maximumWidth: width
    maximumHeight: height
    minimumWidth: width
    minimumHeight: height
    title: "ShoopDaLoop Synced Loops"
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
        
        client_name_hint: "synced_loops"
        update_interval_ms: 30

        AudioPortPair {
            id: audio_ports_l
            input_name_hint: 'audio_in_l'
            output_name_hint: 'audio_out_l'
        }
        AudioPortPair {
            id: audio_ports_r
            input_name_hint: 'audio_in_r'
            output_name_hint: 'audio_out_r'
        }
        MidiPortPair {
            id: midi_ports
            input_name_hint: 'midi_in'
            output_name_hint: 'midi_out'
        }

        Column {
            LoopWidget {
                id: master
                is_in_hovered_scene: false
                is_in_selected_scene: false
                name: "Master"
                master_loop: this
                targeted_loop: null
                direct_port_pairs: [audio_ports_l, audio_ports_r, midi_ports]
            }

            LoopWidget {
                is_in_hovered_scene: false
                is_in_selected_scene: false
                name: "Synced"
                master_loop: master
                targeted_loop: null
                direct_port_pairs: [audio_ports_l, audio_ports_r, midi_ports]
            }
        }
    }
}
