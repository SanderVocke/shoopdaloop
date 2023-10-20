import QtQuick 6.3
import ShoopDaLoop.PythonLogger

Item {
    id: root

    property bool when: true

    readonly property PythonLogger logger : PythonLogger { name: "Frontend.Qml.LuaScript" }

    // Inputs
    property var script_code: null // Set the script code directly
    property string script_name : '<unknown script>' // Set the script name directly
    property var script_path: null // By setting this, name and code will be auto-loaded
    property bool catch_errors: true // If true, errors will be caught and logged, otherwise they will be thrown

    // Set internally
    property var scripting_context: null
    property var accepted_script_code: null

    property bool ready : false

    function accept() {
        if (accepted_script_code !== null) {
            throw new Error("Script code cannot be changed")
        }
        accepted_script_code = script_code
    }

    onScript_codeChanged: {
        if (when) {
            accept()
        }
    }
    onWhenChanged: {
        if (script_code) {
            accept()
        }
    }

    onScript_pathChanged: {
        script_name = script_path
        script_code = file_io.read_file(script_path)
    }

    onAccepted_script_codeChanged: {
        scripting_context = scripting_engine.new_context()
        logger.debug(() => ("Executing script in new context: " + scripting_context))
        logger.trace(() => ("Code: " + accepted_script_code))
        execute(accepted_script_code, true, script_name)
        ready = true
    }

    Component.onDestruction: {
        if (scripting_context) {
            scripting_engine.delete_context(scripting_context)
        }
    }

    function evaluate(expression, script=script_name) {
        if (!ready) return null;
        logger.trace(() => ("Evaluating expression: " + expression + " in context: " + scripting_context))
        return scripting_engine.evaluate(expression, scripting_context, script, true, catch_errors)
    }

    function execute(statement, force=false, script=script_name) {
        if (!ready && !force) return;
        logger.trace(() => ("Executing statement: " + statement + " in context: " + scripting_context))
        scripting_engine.execute(statement, scripting_context, script, true, catch_errors)
    }
}