import QtQuick 6.6
import ShoopDaLoop.PythonLogger

import '../midi.js' as Midi
import '../midi_control.js' as Js

Item {
    id: root
    property bool ready: false
    property bool initialized: false
    property var control_interface: null
    property var lua_engine: _lua_engine

    LuaEngine {
        id: _lua_engine
        ready: false
        function update() {
            if (root.control_interface) {
                create_lua_qobject_interface_as_global('__shoop_control_interface', root.control_interface)
            }
            ready = true
        }
        Component.onCompleted: update()
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
`, 'MidiControl', true, true)

        update_all_handlers()

        root.logger.debug(() => ('MIDI control initialized'))

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
    property Component actionImplementation: QtObject {
        property var action_or_script: null
        property var configuration: null
        property string action_name: 'unknown'

        readonly property var callable: {
            if (action_or_script === null || configuration === null) {
                return null
            }

            root.logger.trace(() => ('Generating script for: ' + JSON.stringify(configuration)))

            let base_script = (typeof action_or_script === 'string') ? action_or_script : action_or_script.script
            let inputs = (typeof action_or_script === 'string') ? {} : action_or_script.inputs

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
            root.logger.trace(() => ('Generated script: ' + script))

            let fn = _lua_engine.evaluate(script, 'MidiControl', true, true)
            let _name = action_name
            return function(msg, port, _fn=fn, name=_name) {
                root.logger.debug(() => (`Running action ${name}`))
                _lua_engine.call(_fn, [msg, port], false)
            }
        }
    }

    readonly property PythonLogger logger: PythonLogger { name: "Frontend.Qml.MidiControl" }

    // Handle a MIDI message received and execute any actions that match.
    function handle_midi(msg, control_port) {
        let try_filter = function(filter, msg) {
            let byte_data = msg[filter[0]]
            let result = byte_data & filter[1]
            return result == filter[2]
        }

        root.logger.debug(() => (`Received: [${msg}]. Matching against ${configuration.contents.length} action items.`))

        let msg_object = Midi.parse_msg(msg)

        for(var i=0; i<configuration.contents.length; i++) {
            const filters = configuration.contents[i].filters
            const action = configuration.contents[i].action
            const condition = configuration.contents[i].condition
            const input_overrides = configuration.contents[i].inputs
            if (!( new Set(filters.map(f => try_filter(f, msg))).has(false) )) {
                root.logger.debug(() => (`Matched MIDI control filter: msg = ${msg}, filters = ${filters}, action = ${action}`))
                if (configuration_handlers.length <= i) {
                    root.logger.error(() => ('MIDI control handlers not up-to-date'))
                }
                configuration_handlers[i].callable(msg_object, control_port ? control_port.lua_interface : null)
            } else {
                root.logger.trace(() => (`No match for MIDI control filter: msg = ${msg}, filters = ${filters}, action = ${action}`))
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
            rval.push(actionImplementation.createObject(root, {
                'action_or_script': action,
                'configuration': conf,
                'action_name': action_name,
            }))
        }
        configuration_handlers = rval
    }
}