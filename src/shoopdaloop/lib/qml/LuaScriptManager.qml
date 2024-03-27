import QtQuick 6.6
import ShoopDaLoop.PythonLogger

Item {
    id: root
    property var control_interface: null
    property var active_scripts: ({})

    property PythonLogger logger: PythonLogger { name: "Frontend.Qml.LuaScriptManager" }

    signal changed()

    Component {
        id: factory
        LuaScriptWithEngine {}
    }

    function start_script(script_path) {
        var script = null
        if (script_path in active_scripts) {
            kill(script_path)
        }

        root.logger.debug("Start script: " + script_path)
        active_scripts[script_path] = factory.createObject(root, {
            'script_path': script_path,
            'control_interface': control_interface,
        })
        changed()
        return script
    }

    function get_status(script_path) {
        if (script_path in active_scripts) {
            let s = active_scripts[script_path]
            return s.ran ?
                s.listening ? 'Listening' : 'Finished' : 'Running'
        } else {
            return 'Inactive'
        }
    }

    function maybe_docstring(script_path) {
        let contents = file_io.read_file(script_path)
        let lines = contents.split("\n")
        let docstring = ""
        var found = false

        for(let line of lines) {
            let m = line.match(/^\s*--\s*(.*)$/)
            if (m) {
                docstring += m[1] + "\n"
                found = true
            } else if (line.match(/^\s*$/)) {
                continue
            } else {
                break
            }
        }

        return found ? docstring : null
    }

    function kill(script_path) {
        root.logger.debug("Killing: " + script_path)
        active_scripts[script_path].stop()
        active_scripts[script_path].destroy()
        delete active_scripts[script_path]
        changed()
    }

    function kill_all() {
        for (let script_path of Object.keys(active_scripts)) {
            kill(script_path)
        }
        active_scripts = ({})
        changed()
    }

    function sync(script_paths) {
        var running = []
        for (let script_path of Object.keys(active_scripts)) {
            // TODO restart if needed
            running.push(script_path)
        }
        for (let script_path of script_paths) {
            if (!running.includes(script_path)) {
                start_script(script_path)
            }
        }
        changed()
    }
}