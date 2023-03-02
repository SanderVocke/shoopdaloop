import Loop
import QtQuick 2.15

Loop {
    property bool loaded: initialized

    onLoadedChanged: if(loaded) { console.log("LOADED: Loop") }
}