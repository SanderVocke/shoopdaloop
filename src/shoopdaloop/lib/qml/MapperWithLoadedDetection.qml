import QtQuick 6.3
import QtQuick.Controls 6.3
import QtQuick.Controls.Material 6.3

Mapper {
    id: root
    property bool loaded : {
        if (instances.length == model.length) {
            for (var i = 0; i < instances.length; i++) {
                if (!instances[i].loaded) { return false }
            }
            return true
        }
        return false
    }
}