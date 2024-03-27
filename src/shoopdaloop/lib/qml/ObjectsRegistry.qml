import QtQuick 6.6

Registry {
    property list<LoopWidget> loop_widgets: []
    property list<TrackWidget> track_widgets: []

    function update_loop_widgets() { loop_widgets = select(e => (e instanceof LoopWidget)) }
    function update_track_widgets() { track_widgets = select(e => (e instanceof TrackWidget)) }

    Component.onCompleted: {
        update_loop_widgets()
        update_track_widgets()
    }
    onContentsChanged: {
        update_loop_widgets()
        update_track_widgets()
    }
}