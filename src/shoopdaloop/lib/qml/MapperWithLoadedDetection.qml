import QtQuick 6.3
import QtQuick.Controls 6.3
import QtQuick.Controls.Material 6.3

Mapper {
    id: root
    property bool loaded : {
        if (unsorted_instances.length == model.length) {
            for (var i = 0; i < unsorted_instances.length; i++) {
                if (!unsorted_instances[i].loaded) { return false }
            }
            return true
        }
        return false
    }
}