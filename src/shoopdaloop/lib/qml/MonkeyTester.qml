import QtQuick 6.3
import QtQuick.Controls 6.3
import QtQuick.Controls.Material 6.3
import ShoopDaLoop.PythonLogger
import ShoopConstants
import "../generate_session.js" as GenerateSession

Item {
    id: root

    property var session: null
    property var tracks: session ? session.main_tracks : []

    property var default_actions_distribution: ({
        'add_track': 1.0,
        'remove_track': 1.0,
        'add_loop': 1.0,
        'loop_start_record': 1.5,
    })

    property var default_track_type_distribution: ({
        'direct': 1.0,
        'drywet': 1.0
    })

    property var actions_distribution: default_actions_distribution
    property var track_type_distribution: default_track_type_distribution
    property real interval: 0.1

    readonly property PythonLogger logger : PythonLogger { name: "Frontend.Qml.MonkeyTester" }

    property bool running: false

    Timer {
        id: timer
        interval: root.interval * 1000
        running: false
        onTriggered: root.action()
    }

    property var sync_loop : registries.state_registry.sync_loop

    function object_values_total(obj) {
        var r = 0.0
        var vals = Object.values(obj)
        for(var i=0; i<vals.length; i++) {
            r += vals[i]
        }
        return r;
    }

    function pick_random(distribution) {
        var choice_val = Math.random() * object_values_total(distribution)
        var values = Object.values(distribution)
        var keys = Object.keys(distribution)
        for(var i=0; i<values.length; i++) {
            if (choice_val <= values[i]) {
                return keys[i]
            } else {
                choice_val -= values[i]
            }
        }
        return keys[values.length-1]
    }

    function pick_random_from_list(list) {
        if(list.length == 0) { return null; }
        var distribution = {}
        for(var i=0; i<list.length; i++) {
            distribution[i] = 1.0
        }
        let idx = pick_random(distribution)
        return list[idx]
    }

    function loops() {
        var rval = []
        for(var i=0; i<root.tracks.length; i++) {
            for (var j=0; j<root.tracks[i].loops.length; j++) {
                rval.push(root.tracks[i].loops[j])
            }
        }
        return rval
    }

    function action_possible(action) {
        if (action == 'add_track') { return root.tracks.length < 5 }
        else if (action == 'add_loop') { return root.tracks.length > 0 }
        else if (action == 'remove_track') { return root.tracks.length > 3 }
        else if (action == 'loop_start_record') { return root.tracks.length > 0 }
        return false;
    }

    function start() {
        root.logger.debug('start')
        root.running = true
        timer.start()
    }

    function action() {
        let sync_loop = session.sync_track.loops[0]
        if (sync_loop.length == 0) {
            sync_loop.set_length(24000)
        }
        sync_loop.transition(ShoopConstants.LoopMode.Playing, 0, false, false)

        var chosen_action = null;
        while(chosen_action == null) {
            let candidate = pick_random(actions_distribution)
            if (action_possible(candidate)) {
                chosen_action = candidate
            }
        }
        root.logger.debug(chosen_action)

        if (chosen_action == 'add_track') { add_track(); }
        else if (chosen_action == 'add_loop') { add_loop(); }
        else if (chosen_action == 'remove_track') { remove_track(); }
        else if (chosen_action == 'loop_start_record') { loop_start_record(); }
        
        if (root.running) { timer.start() }
    }

    function add_track() {
        let name = 't_' + Math.random().toString(36).slice(2, 7)
        var descriptor = null
        
        let track_type = pick_random(track_type_distribution)

        if (track_type == 'direct') {
            root.logger.debug('--> direct')
            descriptor = GenerateSession.generate_default_track(
                name,
                4,
                name,
                root.sync_loop == null,
                name,
                0, //n_audio_dry
                0, //n_audio_wet
                2, //n_audio_direct
                false, //have_midi_dry
                true, //have_midi_direct
                false, //have_drywet_jack_ports
                undefined, //drywet_carla_type
            );
        } else if (track_type == 'drywet') {
            root.logger.debug('--> drywet')
            descriptor = GenerateSession.generate_default_track(
                name,
                4,
                name,
                root.sync_loop == null,
                name,
                2, //n_audio_dry
                2, //n_audio_wet
                0, //n_audio_direct
                true, //have_midi_dry
                false, //have_midi_direct
                false, //have_drywet_jack_ports
                'test2x2x1', //drywet_carla_type
            );
        }
                
        root.session.get_tracks_widget().add_track({
            initial_descriptor: descriptor
        })
    }

    function add_loop() {
        let track = pick_random_from_list(root.tracks)
        root.logger.debug(`--> add to ${track.name}`)
        track.add_default_loop()   
    }

    function remove_track() {
        let track = pick_random_from_list(root.tracks)
        root.logger.debug(`--> delete ${track.name}`)
        track.requestDelete()
    }

    function loop_start_record() {
        let track = pick_random_from_list(root.tracks)
        if (!track) {
            root.logger.debug('--> no track');
            return;
        }
        let loop = pick_random_from_list(track.loops)
        if (!loop) {
            root.logger.debug('--> no loop');
            return;
        }
        loop.record_n(0, 4)
    }
}