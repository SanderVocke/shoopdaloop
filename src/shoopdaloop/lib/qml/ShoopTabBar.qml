import QtQuick 6.6
import QtQuick.Controls 6.6
import QtQuick.Controls.Material 6.6

Row {
    id: root
    height: childrenRect.height
    width: childrenRect.width

    property var buttons : Array.from(children).filter(c => c instanceof ShoopTabButton)
    readonly property int currentIndex: {
        for (var i=0; i<buttons.length; i++) {
            if (selected_button_representative === buttons[i].representative) { return i; }
        }
        return -1
    }
    readonly property var selected_button: buttons[currentIndex]
    property var selected_button_representative: null

    onButtonsChanged: {
        if (currentIndex < 0 && buttons.length > 0) {
            for (var i = 0; i<buttons.length; i++) {
                if (buttons[i].auto_select) { select(buttons[i]); break; }
            }
        }
    }

    function select(btn) {
        selected_button_representative = btn.representative
    }
    function select_representative(r) {
        selected_button_representative = r
    }
    function select_idx(idx) {
        select(buttons[idx])
    }
}