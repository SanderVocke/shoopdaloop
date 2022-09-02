import QtQuick 6.3
import QtQuick.Controls 6.3
import QtQuick.Controls.Material 6.3

import "../generate_session.js" as GenerateSession

// Dialog for adding a new track
Dialog {
    id: newtrackdialog
    standardButtons: Dialog.Ok | Dialog.Cancel
    parent: Overlay.overlay
    x: (parent.width - width) / 2
    y: (parent.height - height) / 2
    modal: true
    title: 'Add track'

    width: 400
    height: 550

    property alias track_name : name_field.text
    property alias is_drywet : select_type.is_drywet
    property alias is_drywet_carla : select_processing_kind.is_carla
    property alias drywet_carla_type : select_processing_kind.carla_type
    property alias is_drywet_jack : select_processing_kind.is_jack
    readonly property int n_direct_audio_channels : is_drywet ? 0 : custom_direct_audio_channels.value
    readonly property int n_dry_audio_channels : is_drywet ? custom_dry_audio_channels.value : 0
    readonly property int n_wet_audio_channels : is_drywet ? custom_wet_audio_channels.value : 0
    readonly property bool has_direct_midi : is_drywet ? false : direct_midi_checkbox.checked
    readonly property bool has_dry_midi : is_drywet ? dry_midi_checkbox.checked : false
    property string port_name_base : track_name.toLowerCase().replace(' ', '_')

    function open_for_new_track() {
        var idx = root.tracks.length
        track_name = "Track " + idx.toString()
        open()
    }

    signal addTrackDescriptor(var desc)

    onAccepted: {
        var track_descriptor = GenerateSession.generate_default_track(
            track_name,
            Math.max(8, root.max_slots()),
            port_name_base,
            false,
            port_name_base,
            n_dry_audio_channels,
            n_wet_audio_channels,
            n_direct_audio_channels,
            has_dry_midi,
            has_direct_midi,
            is_drywet && is_drywet_jack,
            (is_drywet && is_drywet_carla) ? drywet_carla_type : undefined
        )
        addTrackDescriptor(track_descriptor)
    }

    Grid {
        columns: 2
        verticalItemAlignment: Grid.AlignVCenter
        columnSpacing: 10

        Label {
            text: "Name:"
        }
        ShoopTextField {
            id: name_field
        }

        Label {
            text: "Type:"
        }
        ShoopComboBox {
            id: select_type
            textRole: "text"
            valueRole: "value"
            model: [
                { value: 'direct', text: "Direct" },
                { value: 'drywet', text: "Dry + Wet" },
            ]
            property bool is_drywet : currentValue == 'drywet'
        }

        // START CONTROLS FOR DIRECT LOOP TYPE

        Label {
            text: "Audio:"
            visible: !is_drywet
        }
        AudioTypeSelect {
            id: select_direct_audio_type
            visible: !is_drywet
            onChangeNChannels: (n) => custom_direct_audio_channels.value = n
        }

        Label {
            text: "Audio channels:"
            visible: !is_drywet && select_direct_audio_type.show_custom
        }
        SpinBox {
            id: custom_direct_audio_channels
            visible: !is_drywet && select_direct_audio_type.show_custom
            value: 2
            from: 0
            to: 10
            width: 120
        }

        Label {
            text: "MIDI:"
            visible: !is_drywet
        }
        CheckBox {
            id: direct_midi_checkbox
            tristate: false
            visible: !is_drywet
        }

        // START CONTROLS FOR DRY/WET LOOP TYPE

        Label {
            text: "Processing Kind:"
            visible: is_drywet
        }
        ShoopComboBox {
            id: select_processing_kind
            visible: is_drywet
            textRole: "text"
            valueRole: "value"
            model: [
                { value: 'jack', text: "External (JACK)" },
                { value: 'carla_rack', text: "Carla (Rack)" },
                { value: 'carla_patchbay', text: "Carla (Patchbay)" },
                { value: 'carla_patchbay_16', text: "Carla (Patchbay 16x)" }
            ]
            property bool is_carla : ['carla_rack', 'carla_patchbay', 'carla_patchbay_16'].includes(currentValue)
            property bool is_jack : currentValue == 'jack'
            property var carla_type : is_carla ? currentValue : undefined
        }

        Label {
            text: "Dry audio:"
            visible: is_drywet
        }
        AudioTypeSelect {
            id: select_dry_audio_type
            visible: is_drywet
            onChangeNChannels: (n) => custom_dry_audio_channels.value = n
        }

        Label {
            text: "Dry audio channels:"
            visible: is_drywet && select_dry_audio_type.show_custom
        }
        SpinBox {
            id: custom_dry_audio_channels
            visible: is_drywet && select_dry_audio_type.show_custom
            value: 2
            from: 0
            to: 10
            width: 120
        }

        Label {
            text: "Dry MIDI:"
            visible: is_drywet
        }
        CheckBox {
            id: dry_midi_checkbox
            tristate: false
            visible: is_drywet
        }

        Label {
            text: "Wet audio:"
            visible: is_drywet
        }
        AudioTypeSelect {
            id: select_wet_audio_type
            visible: is_drywet
            onChangeNChannels: (n) => custom_wet_audio_channels.value = n
        }

        Label {
            text: "Wet audio channels:"
            visible: is_drywet && select_wet_audio_type.show_custom
        }
        SpinBox {
            id: custom_wet_audio_channels
            visible: is_drywet && select_wet_audio_type.show_custom
            value: 2
            from: 0
            to: 10
            width: 120
        }
    }

    component AudioTypeSelect : ShoopComboBox {
        id: select_audio_type
        textRole: "text"
        valueRole: "value"
        model: [
            { value: 'disabled', text: "Disabled" },
            { value: 'mono', text: "Mono" },
            { value: 'stereo', text: "Stereo" },
            { value: 'custom', text: "Custom" }
        ]
        currentIndex: 2
        property bool show_custom : { currentValue == 'custom' }
        signal changeNChannels(int n)
        function update() {
            switch(currentValue) {
                case 'disabled':
                    changeNChannels(0);
                    break;
                case 'mono':
                    changeNChannels(1);
                    break;
                case 'stereo':
                    changeNChannels(2);
                    break;
                default:
                    break;
            }
        }
        Component.onCompleted: update()
        onActivated: update()
    }
}