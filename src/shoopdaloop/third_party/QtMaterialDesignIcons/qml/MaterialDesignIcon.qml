import QtQuick 2.0
import "MaterialDesignIconGlyphs.js" as MaterialGlyphs

Item {
    property int size: 24
    property string name
    property color color

    width: size
    height: size

    Text {
        anchors.fill: parent

        color: parent.color

        font.family: materialFont.name
        font.pixelSize: parent.height

        text: MaterialGlyphs.glyphs[parent.name]
    }

    FontLoader {
        id: materialFont
        source: "qrc:/materialdesignicons-webfont.ttf"
    }
}
