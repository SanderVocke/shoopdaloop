import QtQuick 6.3
import QtQuick.Controls 6.3
import QtQuick.Layouts 6.3
import QtQuick.Controls.Material 6.3
import ShoopDaLoop.PythonMIDIControlDialect 1.0
import ShoopDaLoop.PythonMIDIControlInputRule 1.0
import Qt.labs.qmlmodels 1.0

import '../../build/StatesAndActions.js' as StatesAndActions

Dialog {
    id: dialog
    modal: true
    standardButtons: Dialog.Close

    width: 800
    height: 500
}
