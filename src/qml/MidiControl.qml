import QtQuick 6.6

import 'js/midi.js' as Midi
import 'js/midi_control.js' as Js

Item {
    id: root
    property bool ready: false
    property bool initialized: false
    property var control_interface: null
    property var lua_engine: _lua_engine

    LuaEngine {
        id: _lua_engine
        property bool ready: false
        Component.onCompleted: {
            control_interface.install_on_lua_engine(lua_engine)
            ready = true
        }
    }

    function initialize() {
        if (!lua_engine) { return; }
        if (!lua_engine.ready) { return; }
        if (initialized) { return; }

        // Include all the shoop libraries so that their functions can be used
        // without having to require them explicitly.
        _lua_engine.execute(`
shoop_control = require('shoop_control')
shoop_coords = require('shoop_coords')
shoop_helpers = require('shoop_helpers')
shoop_format = require('shoop_format')
`, 'MidiControl', true)

        update_all_handlers()

        root.logger.debug('MIDI control initialized')

        ready = true
        initialized = true
    }

    Component.onCompleted: initialize()
    Connections {
        target: _lua_engine
        function onReadyChanged() { root.initialize() }
    }
    onConfigurationChanged: update_all_handlers()

    // The configuration is a list of [ [...filters], action ]
    // TODO: some optimization in the implementation so that we don't have to iterate over all the filters
    // for every single message
    property var configuration : MidiControlConfiguration {}

    // Hook up an action descriptor to a configuration descriptor using that action,
    // generating a callable that can be called to execute the configured action.
    function create_handler(action, configuration, name) {
        if (action === null || configuration === null) {
            return null
        }

        root.logger.trace('Generating script for: ' + JSON.stringify(configuration))

        let base_script = (typeof action === 'string') ? action : action.script
        let inputs = (typeof action === 'string') ? {} : action.inputs

        var script = `return function(msg, port) `
        let do_if =  ('condition' in configuration && configuration.condition)
        if (do_if) {
            script += `if (${configuration.condition}) then `
        }
        for (const input_name in inputs) {
            let action_input = inputs[input_name]
            var input_impl = null
            let input_to_use = ('inputs' in configuration && input_name in configuration.inputs) ? configuration.inputs[input_name] : action_input.default
            if (action_input.hasOwnProperty('presets') && input_to_use in action_input.presets) {
                input_impl = action_input.presets[input_to_use]
            } else {
                input_impl = input_to_use
            }
            script += `local ${input_name} = ${input_impl}; `
        }
        script += ` ${base_script} `
        if (do_if) { script += `end ` }
        script += ` end`
        root.logger.trace('Generated script: ' + script)

        let cb = _lua_engine.create_qt_to_lua_callback_fn(
            `MidiControl-${name}`,
            script
        )

        let _name = name
        return function(msg, port, _cb=cb, name=_name) {
            root.logger.debug(`Running action ${name}`)
            cb.call_with_arg({
                'msg': msg,
                'port': port
            })
        }
    }

    readonly property ShoopRustLogger logger: ShoopRustLogger { name: "Frontend.Qml.MidiControl" }

    // Handle a MIDI message received and execute any actions that match.
    function handle_midi(msg, control_port) {
        let try_filter = function(filter, msg) {
            let byte_data = msg[filter[0]]
            let result = byte_data & filter[1]
            return result == filter[2]
        }

        root.logger.debug(`Received: [${msg}]. Matching against ${configuration.contents.length} action items.`)

        let msg_object = Midi.parse_msg(msg)

        for(var i=0; i<configuration.contents.length; i++) {
            const filters = configuration.contents[i].filters
            const action = configuration.contents[i].action
            const condition = configuration.contents[i].condition
            const input_overrides = configuration.contents[i].inputs
            if (!( new Set(filters.map(f => try_filter(f, msg))).has(false) )) {
                root.logger.debug(`Matched MIDI control filter: msg = ${msg}, filters = ${filters}, action = ${action}`)
                if (configuration_handlers.length <= i) {
                    root.logger.error('MIDI control handlers not up-to-date')
                }
                configuration_handlers[i](msg_object, control_port ? control_port.lua_interface : null)
            } else {
                root.logger.trace(`No match for MIDI control filter: msg = ${msg}, filters = ${filters}, action = ${action}`)
            }
        }
    }

    property var configuration_handlers: []
    function update_all_handlers() {
        var rval = []
        for (var i=0; i<configuration.contents.length; i++) {
            let conf = configuration.contents[i]
            let action = (conf.action in Js.builtin_actions) ? Js.builtin_actions[conf.action] : conf.action
            let action_name = (conf.action in Js.builtin_actions) ? conf.action : '<custom>'
            rval.push(create_handler(action, conf, action_name))
        }
        configuration_handlers = rval
    }
}