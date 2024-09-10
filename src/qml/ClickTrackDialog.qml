import QtQuick 6.6
import QtQuick.Controls 6.6
import QtQuick.Controls.Material 6.6

// The click track dialog allows the user to interactively configure, preview
// and select a generated click track clip.
Dialog {
    id: root
    modal: true
    standardButtons: Dialog.Ok | Dialog.Cancel

    width: 400
    height: 450

    property var loop : null

    property bool audio_enabled: true
    property bool midi_enabled: true

    property alias primary_click: primary_click_combo.currentText
    property alias secondary_click: secondary_click_combo.currentText
    property alias click_note_text: click_note_field.text
    property alias note_length_text: note_length_field.text
    property alias bpm_text: bpm_field.text
    property alias n_beats_text: n_beats_field.text
    property alias num_secondary_per_primary_text: secondary_clicks_per_primary_field.text
    property alias alternate_delay_percent_text: alternate_delay_percent_field.text
    property var possible_primary_clicks: {
        if(click_track_generator) {
            return click_track_generator.get_possible_clicks()
        }
        return []
    }
    property var possible_secondary_clicks: {
        if(click_track_generator) {
            return ['None'].concat(click_track_generator.get_possible_clicks());
        }
        return []
    }

    property real bpm: parseFloat(bpm_text)
    property int n_beats: parseInt(n_beats_text)
    property int alternate_delay_percent: parseInt(alternate_delay_percent_text)

    property int click_note : parseInt(click_note_text)
    property real note_length : parseFloat(note_length_text)

    function generate() {
        if (kind_combo.currentValue == 'Audio') {
            var clicks = [primary_click]
            if (secondary_click != 'None') {
                for (var i = 0; i < parseInt(num_secondary_per_primary_text); i++) {
                    clicks.push(secondary_click)
                }
            }

            return {
                'filename': click_track_generator.generate_audio(clicks, bpm, n_beats, alternate_delay_percent),
                'kind': 'audio'
            }
        } else if (kind_combo.currentValue == 'Midi') {
            return {
                'filename': click_track_generator.generate_midi(
                        [click_note],
                        [0],
                        [127],
                        note_length, bpm, n_beats, alternate_delay_percent
                    ),
                'kind': 'midi'
            }
        }
    }

    signal acceptedClickTrack(kind: string, filename: string)

    Grid {
        columns: 2
        spacing: 5
        verticalItemAlignment: Grid.AlignVCenter
        horizontalItemAlignment: Grid.AlignRight

        Text {
            text: "Kind:"
            color: Material.foreground
            verticalAlignment: Text.AlignVCenter
        }
        ShoopComboBox {
            width: 150
            id: kind_combo
            model: {
                var rval = []
                if (root.audio_enabled) { rval.push('Audio'); }
                if (root.midi_enabled) { rval.push('Midi'); }
                return rval;
            }
            currentIndex: 0
        }

        Text {
            text: "Primary click:"
            color: Material.foreground
            verticalAlignment: Text.AlignVCenter
            visible: kind_combo.currentValue == 'Audio'
        }
        ShoopComboBox {
            width: 150
            id: primary_click_combo
            model: root.possible_primary_clicks
            currentIndex: 0
            visible: kind_combo.currentValue == 'Audio'
        }

        Text {
            text: "Secondary click:"
            color: Material.foreground
            verticalAlignment: Text.AlignVCenter
            visible: kind_combo.currentValue == 'Audio'
        }
        ShoopComboBox {
            width: 150
            id: secondary_click_combo
            model: root.possible_secondary_clicks
            currentIndex: 2
            visible: kind_combo.currentValue == 'Audio'
        }

        Text {
            text: "Click MIDI note:"
            color: Material.foreground
            verticalAlignment: Text.AlignVCenter
            visible: kind_combo.currentValue == 'Midi'
        }
        ShoopTextField {
            width: 150
            id: click_note_field
            text: "64"
            validator: IntValidator { bottom: 0; top: 127 }
            visible: kind_combo.currentValue == 'Midi'
        }

        Text {
            text: "Note length (s):"
            color: Material.foreground
            verticalAlignment: Text.AlignVCenter
            visible: kind_combo.currentValue == 'Midi'
        }
        ShoopTextField {
            width: 150
            id: note_length_field
            text: "0.1"
            validator: DoubleValidator { bottom: 0.0; top: 10.0 }
            visible: kind_combo.currentValue == 'Midi'
        }

        Text {
            text: "Secondary clicks per primary:"
            color: Material.foreground
            verticalAlignment: Text.AlignVCenter
        }
        ShoopTextField {
            width: 150
            id: secondary_clicks_per_primary_field
            text: "3"
            validator: IntValidator { bottom: 0 }
        }

        Text {
            text: "Clicks per minute:"
            color: Material.foreground
            verticalAlignment: Text.AlignVCenter
        }
        ShoopTextField {
            width: 150
            id: bpm_field
            text: "100"
            validator: DoubleValidator { bottom: 1.0 }
        }

        Text {
            text: "Number of clicks:"
            color: Material.foreground
            verticalAlignment: Text.AlignVCenter
        }
        ShoopTextField {
            width: 150
            id: n_beats_field
            text: "4"
            validator: IntValidator { bottom: 1 }
        }

        Text {
            text: "Delay odd clicks by (%):"
            color: Material.foreground
            verticalAlignment: Text.AlignVCenter
        }
        ShoopTextField {
            width: 150
            id: alternate_delay_percent_field
            text: "0"
            validator: IntValidator { bottom: 0; top: 100 }
        }

        ExtendedButton {
            text: "Fill loop length"
            tooltip: "Sets clicks per minute so that the chosen # of clicks fits the current loop length."
            onClicked: () => {
                if (root.loop) {
                    root.loop.create_backend_loop()
                    var srate = root.loop.maybe_loaded_loop.get_backend().get_sample_rate()
                    var _bpm = n_beats / (root.loop.length / srate / 60.0)
                    bpm_field.text = _bpm.toFixed(2)
                }
            }
        }

        ExtendedButton {
            tooltip: "Listen to a preview of the chosen click track."
            text: "Preview"
            enabled: kind_combo.currentValue == 'Audio'
            onClicked: () => {
                           var out = generate()
                           if (out.kind != 'audio') {
                                throw new Error("Preview only supported for audio click tracks")
                           }
                           click_track_generator.preview(out.filename)
                       }
        }
    }

    onAccepted: () => {
        var result = generate()
        acceptedClickTrack(result.kind, result.filename)
    }
}
