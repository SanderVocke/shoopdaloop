import QtQuick 2.15
import QtQuick.Controls 2.15
import QtQuick.Controls.Material 2.15
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

        LoopWidget {
            is_in_hovered_scene: false
            is_in_selected_scene: false
            name: "Loop"
            master_loop: null
            targeted_loop: null
            direct_port_pairs: [audio_ports_l, audio_ports_r, midi_ports]
        }
    }
}
