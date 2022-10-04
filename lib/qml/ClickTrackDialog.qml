import QtQuick 2.15
import QtQuick.Controls 2.15
import QtQuick.Controls.Material 2.15

// The click track dialog allows the user to interactively configure, preview
// and select a generated click track clip.
Dialog {
    id: dialog
    modal: true
    standardButtons: Dialog.Ok | Dialog.Cancel

    width: 400
    height: 400

    property alias primary_click: primary_click_combo.currentText
    property alias secondary_click: secondary_click_combo.currentText
    property alias bpm_text: bpm_field.text
    property alias n_beats_text: n_beats_field.text
    property alias num_secondary_per_primary_text: secondary_clicks_per_primary_field.text
    property alias alternate_delay_percent_text: alternate_delay_percent_field.text
    property var possible_primary_clicks: click_track_generator.get_possible_clicks()
    property var possible_secondary_clicks: ['None'].concat(click_track_generator.get_possible_clicks())

    function generate() {
        var clicks = [primary_click]
        if (secondary_click != 'None') {
            for (var i = 0; i < parseInt(num_secondary_per_primary_text); i++) {
                clicks.push(secondary_click)
            }
        }

        return click_track_generator.generate(clicks, parseInt(bpm_text), parseInt(n_beats_text), parseInt(alternate_delay_percent_text));
    }

    signal acceptedClickTrack(filename: string)

    Column {
        Row {
            Text {
                text: "Primary click:"
            }
            ComboBox {
                id: primary_click_combo
                model: dialog.possible_primary_clicks
                currentIndex: 0
            }
        }

        Row {
            Text {
                text: "Clicks per minute:"
            }
            TextField {
                id: bpm_field
                text: "100"
                validator: IntValidator { bottom: 1 }
            }
        }

        Row {
            Text {
                text: "Number of clicks:"
            }
            TextField {
                id: n_beats_field
                text: "4"
                validator: IntValidator { bottom: 1 }
            }
        }

        Row {
            Text {
                text: "Delay odd clicks by (%):"
            }
            TextField {
                id: alternate_delay_percent_field
                text: "0"
                validator: IntValidator { bottom: 0; top: 100 }
            }
        }

        Row {
            Text {
                text: "Secondary click:"
            }
            ComboBox {
                id: secondary_click_combo
                model: dialog.possible_secondary_clicks
                currentIndex: 0
            }
        }

        Row {
            Text {
                text: "Secondary clicks per primary:"
            }
            TextField {
                id: secondary_clicks_per_primary_field
                text: "3"
                validator: IntValidator { bottom: 0 }
            }
        }

        Button {
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
