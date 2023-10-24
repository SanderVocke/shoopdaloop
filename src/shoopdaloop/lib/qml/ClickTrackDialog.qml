import QtQuick 6.3
import QtQuick.Controls 6.3
import QtQuick.Controls.Material 6.3

// The click track dialog allows the user to interactively configure, preview
// and select a generated click track clip.
Dialog {
    id: dialog
    modal: true
    standardButtons: Dialog.Ok | Dialog.Cancel

    width: 400
    height: 450

    property var loop : null

    property alias primary_click: primary_click_combo.currentText
    property alias secondary_click: secondary_click_combo.currentText
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

    function generate() {
        var clicks = [primary_click]
        if (secondary_click != 'None') {
            for (var i = 0; i < parseInt(num_secondary_per_primary_text); i++) {
                clicks.push(secondary_click)
            }
        }

        return click_track_generator.generate(clicks, bpm, n_beats, alternate_delay_percent);
    }

    signal acceptedClickTrack(filename: string)

    Grid {
        columns: 2
        spacing: 5
        verticalItemAlignment: Grid.AlignVCenter
        horizontalItemAlignment: Grid.AlignRight

        Text {
            text: "Primary click:"
            color: Material.foreground
            verticalAlignment: Text.AlignVCenter
        }
        ShoopComboBox {
            width: 150
            id: primary_click_combo
            model: dialog.possible_primary_clicks
            currentIndex: 0
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

        Text {
            text: "Secondary click:"
            color: Material.foreground
            verticalAlignment: Text.AlignVCenter
        }
        ShoopComboBox {
            width: 150
            id: secondary_click_combo
            model: dialog.possible_secondary_clicks
            currentIndex: 0
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

        ExtendedButton {
            text: "Fill loop length"
            tooltip: "Sets clicks per minute so that the chosen # of clicks fits the current loop length."
            onClicked: () => {
                if (dialog.loop) {
                    dialog.loop.create_backend_loop()
                    var srate = dialog.loop.maybe_loaded_loop.get_backend().get_sample_rate()
                    var _bpm = n_beats / (dialog.loop.length / srate / 60.0)
                    bpm_field.text = _bpm.toFixed(2)
                }
            }
        }

        ExtendedButton {
            tooltip: "Listen to a preview of the chosen click track."
            text: "Preview"
            onClicked: () => {
                           var out = generate()
                           click_track_generator.preview(out)
                       }
        }
    }

    onAccepted: () => {
                    acceptedClickTrack(generate())
                }
}
