import QtQuick 6.6

Item {
    id: root

    visible: false

    property var canvas: null
    property var selectedLoops: []
    property int generation: 0
    property var fetchedChannels: new Set()
    property var runningFetch: null
    readonly property var selectedLoop: selectedLoops.length > 0 ? selectedLoops[0] : null
    readonly property var loopBackend: selectedLoop ? selectedLoop.maybe_loop : null
    readonly property var channels: loopBackend && Array.isArray(loopBackend.channels)
        ? loopBackend.channels.filter(channel => channel
            && channel.descriptor
            && channel.descriptor.type === "audio")
        : []

    function pushState() {
        if (!canvas) return
        const loading = selectedLoop
            && (!loopBackend || fetchedChannels.size < channels.length)
        canvas.setDetailsState(
            generation,
            selectedLoop !== null,
            selectedLoop ? selectedLoop.name : "",
            loading,
            channels.length
        )
    }

    function pushChannelState(index) {
        if (!canvas || index < 0 || index >= channels.length) return
        const channel = channels[index]
        const played = channel.last_played_sample === null
            || channel.last_played_sample === undefined
            ? -1 : channel.last_played_sample
        canvas.setDetailsChannelState(
            generation,
            index,
            channel.obj_id,
            channel.start_offset,
            selectedLoop ? selectedLoop.length : 0,
            played
        )
    }

    function restart() {
        generation += 1
        fetchedChannels = new Set()
        pushState()
        channels.forEach((channel, index) => pushChannelState(index))
        fetchNext()
    }

    function fetchNext() {
        if (!canvas || runningFetch || !loopBackend) return
        let index = -1
        for (let candidate = 0; candidate < channels.length; candidate += 1) {
            if (!fetchedChannels.has(candidate) || channels[candidate].data_dirty) {
                index = candidate
                break
            }
        }
        if (index < 0) {
            pushState()
            return
        }

        const token = generation
        const channel = channels[index]
        canvas.setDetailsDataTarget(token, index)
        runningFetch = channel.get_data_async_and_send_to(
            canvas,
            "setDetailsChannelData(QVariant)"
        )
        runningFetch.then(success => {
            if (token === generation && success) {
                const updated = new Set(fetchedChannels)
                updated.add(index)
                fetchedChannels = updated
                channel.clear_data_dirty()
                pushChannelState(index)
                pushState()
            }
            runningFetch = null
            fetchNext()
        })
    }

    onSelectedLoopChanged: restart()
    onChannelsChanged: restart()

    Timer {
        interval: 200
        repeat: true
        running: root.selectedLoop !== null
        onTriggered: {
            root.channels.forEach((channel, index) => root.pushChannelState(index))
            root.fetchNext()
        }
    }

    Component.onCompleted: restart()
}
