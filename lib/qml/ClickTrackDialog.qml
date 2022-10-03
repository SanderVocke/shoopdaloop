import QtQuick 2.15
import QtQuick.Controls 2.15
import QtQuick.Controls.Material 2.15

// The click track dialog allows the user to interactively configure, preview
// and select a generated click track clip.
Dialog {
    id: dialog
    modal: true
    standardButtons: Dialog.Ok | Dialog.Cancel

    property string primary_click: 'click_high'
    property string secondary_click: 'click_low'
    property int bpm: 100
    property int n_beats: 4
    property int num_secondary_per_primary: 0
    property int alternate_delay_percent: 0

    function generate() {
        var clicks = [primary_click]
        for (var i = 0; i < num_secondary_per_primary; i++) {
            clicks.append(secondary_click)
        }

        return click_track_generator.generate(clicks, bpm, n_beats, alternate_delay_percent);
    }

    signal acceptedClickTrack(filename: string)

    Button {
        text: "Preview"
        onClicked: () => {
                       var out = generate()
                       click_track_generator.preview(out)
                   }
    }

    onAccepted: () => {
                    acceptedClickTrack(generate())
                }
}
