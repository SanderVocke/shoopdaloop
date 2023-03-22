import QtQuick 2.15
import QtQuick.Controls 2.15
import QtQuick.Controls.Material 2.15

Repeater {
    id: root
    property bool loaded : false
    property int n_loaded

    function all_items() { return Array.from(Array(count).keys()).map((i) => itemAt(i)) }

    function item_loaded_changed(item) {
        if (item.loaded) { n_loaded = n_loaded + 1 }
        else { n_loaded = n_loaded - 1 }
    }

    function rebind() {
        var _n_loaded = 0
        all_items().map((item) => {
            if (item) {
                if(item.loaded) { _n_loaded++; }
                item.onLoadedChanged.connect(() => item_loaded_changed(item))
            }
        })
        n_loaded = _n_loaded
        if (_n_loaded == count) { loaded = true }
        else { loaded = Qt.binding(() => { return n_loaded == count }) }
    }

    Component.onCompleted: rebind()
    onItemAdded: rebind()
    onItemRemoved: rebind()
}