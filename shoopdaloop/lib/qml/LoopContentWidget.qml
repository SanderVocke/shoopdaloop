import QtQuick 2.15
import QtQuick.Controls 2.15
import QtQuick.Controls.Material 2.15

import '../mode_helpers.js' as ModeHelpers

Item {
    id: root
    property var loop
    property var master_loop

    enum Tool {
        SetStartOffsetAll,
        SetStartOffsetSingle,
        SetLength
    }

    Row {
        id: toolbar_1
        anchors {
            top: parent.top
            left: parent.left
        }
        height: 40
        spacing: 5

        ExtendedButton {
            tooltip: "Re-fetch and render loop data."
            height: 35
            width: 30
            //onClicked: root.request_update_data()

            MaterialDesignIcon {
                size: Math.min(parent.width, parent.height) - 10
                anchors.centerIn: parent
                name: 'refresh'
                color: Material.foreground
            }
        }

        Label {
            anchors.verticalCenter: zoom_combo.verticalCenter
            text: "Samples/pixel:"
        }

        // Slider {
        //     id: zoom_slider
        //     width: 150
        //     value: 0.0
        //     anchors {
        //         verticalCenter: parent.verticalCenter
        //     }
        // }

        ComboBox {
            id: zoom_combo
            model: ['1', '2', '4', '8', '16', '32', '64', '128', '256', '512', '1024', '2048', '4096']
            currentIndex: 8
        }

        Label {
            anchors.verticalCenter: zoom_combo.verticalCenter
            text: "Tool:"
        }

        ComboBox {
            id: tool_combo
            anchors.verticalCenter: zoom_combo.verticalCenter
            textRole: "text"
            valueRole: "value"
            width: 120

            model: [
                { value: LoopContentWidget.Tool.SetStartOffsetAll, text: "set start (all)" },
                { value: LoopContentWidget.Tool.SetStartOffsetSingle, text: "set start (single)" },
                { value: LoopContentWidget.Tool.SetLength, text: "set length" }
            ]
        }

        Label {
            anchors.verticalCenter: zoom_combo.verticalCenter
            text: "Grid:"
        }

        ComboBox {
            id: minor_grid_divider
            anchors.verticalCenter: zoom_combo.verticalCenter
            textRole: "text"
            valueRole: "value"
            currentIndex : 5
            width: 120

            model: [
                { value: undefined, text: "None" },
                { value: 2, text: "Master / 2" },
                { value: 3, text: "Master / 3" },
                { value: 4, text: "Master / 4" },
                { value: 6, text: "Master / 6" },
                { value: 8, text: "Master / 8" },
                { value: 16, text: "Master / 16" },
            ]
        }
    }

    Row {
        id: toolbar_2
        anchors {
            top: toolbar_1.bottom
            left: parent.left
        }
        height: 40
        spacing: toolbar_1.spacing

        Label {
            text: "length:"
            anchors.verticalCenter: length_field.verticalCenter
        }

        TextField {
            id: length_field
            validator: IntValidator {}
            text: root.loop.length.toString()
            onEditingFinished: {
                root.loop.set_length(parseInt(text))
                text = Qt.binding(() => root.loop.length.toString())
            }
        }

        ExtendedButton {
            tooltip: "Additional options."
            height: 35
            width: 30
            onClicked: { console.log ("Unimplemented") }

            anchors.verticalCenter: length_field.verticalCenter

            MaterialDesignIcon {
                size: Math.min(parent.width, parent.height) - 10
                anchors.centerIn: parent
                name: 'dots-vertical'
                color: Material.foreground
            }
        }
    }

    Item {
        anchors {
            top: toolbar_2.bottom
            bottom: parent.bottom
            left: parent.left
            right: parent.right
        }

        Column {
            anchors.fill: parent
            Mapper {
                model: loop.channels

                Item {
                    property int index
                    property var mapped_item

                    height: 50
                    anchors {
                        left: parent.left
                        right: parent.right
                    }

                    Rectangle {
                        anchors.fill: parent
                        color: 'red'

                        Rectangle {
                            id: gradient_rect
                            width: 256
                            height: 256
                            gradient: Gradient {
                                orientation: Gradient.Horizontal
                                GradientStop { position: 0; color: "white" }
                                GradientStop { position: 1; color: "black" }
                            }
                        }

                        ShaderEffectSource {
                            id: shader_source
                            hideSource: true
                            width: 256
                            height: 256
                            sourceItem: gradient_rect
                        }

                        ShaderEffect {
                            anchors.fill: parent
                            property var audio: shader_source
                            fragmentShader: '../../../build/shoopdaloop/lib/qml/shaders/loop_channel.frag.qsb'
                        }
                    }
                }
            }
        }
    }    
}