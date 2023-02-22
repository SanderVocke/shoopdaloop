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
            id: audio_ports_dry_l
            input_name_hint: 'audio_dry_in_l'
            output_name_hint: 'audio_dry_send_l'
        }
        AudioPortPair {
            id: audio_ports_dry_r
            input_name_hint: 'audio_dry_in_r'
            output_name_hint: 'audio_dry_send_r'
        }
        AudioPortPair {
            id: audio_ports_wet_l
            input_name_hint: 'audio_wet_return_l'
            output_name_hint: 'audio_wet_out_l'
        }
        AudioPortPair {
            id: audio_ports_wet_r
            input_name_hint: 'audio_wet_return_r'
            output_name_hint: 'audio_wet_out_r'
        }
        AudioPortPair {
            id: audio_ports_direct_l
            input_name_hint: 'audio_direct_in_l'
            output_name_hint: 'audio_direct_out_l'
        }
        AudioPortPair {
            id: audio_ports_direct_r
            input_name_hint: 'audio_direct_in_r'
            output_name_hint: 'audio_direct_out_r'
        }
        MidiPortPair {
            id: midi_ports_dry
            input_name_hint: 'midi_dry_in'
            output_name_hint: 'midi_dry_send'
        }
        MidiPortPair {
            id: midi_ports_wet
            input_name_hint: 'midi_wet_return'
            output_name_hint: 'midi_wet_out'
        }
        MidiPortPair {
            id: midi_ports_direct
            input_name_hint: 'midi_direct_in'
            output_name_hint: 'midi_direct_out'
        }

        LoopWidget {
            is_in_hovered_scene: false
            is_in_selected_scene: false
            name: "Loop"
            master_loop: null
            targeted_loop: null
            direct_port_pairs: [audio_ports_direct_l, audio_ports_direct_r, midi_ports_direct]
            dry_port_pairs: [audio_ports_dry_l, audio_ports_dry_r, midi_ports_dry]
            wet_port_pairs: [audio_ports_wet_l, audio_ports_wet_r, midi_ports_wet]
        }
    }
}
