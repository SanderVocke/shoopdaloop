import QtQuick 6.6
import ShoopDaLoop.PythonLogger

Item {
    id: root

    property bool when: true

    readonly property PythonLogger logger : PythonLogger { name: "Frontend.Qml.LuaScript" }

    // Inputs
    property var lua_engine: null
    property var script_code: null // Set the script code directly
    property string script_name : '<unknown script>' // Set the script name directly
    property var script_path: null // By setting this, name and code will be auto-loaded
    property bool catch_errors: true // If true, errors will be caught and logged, otherwise they will be thrown

    // Set internally
    property var accepted_script_code: null

    property bool ready : false

    signal ranScript()

    function accept() {
        if (accepted_script_code !== null) {
            throw new Error("Script code cannot be changed")
        }
        accepted_script_code = script_code
    }
    function update() {
        if (script_code !== null && accepted_script_code === null && when && lua_engine) {
            accept()
        }
    }

    onScript_codeChanged: update()
    onWhenChanged: update()
    onLua_engineChanged: update()

    onScript_pathChanged: {
        if (script_path) {
            script_name = script_path
            script_code = file_io.read_file(script_path)
        }
    }

    onAccepted_script_codeChanged: {
        logger.trace(() => ("Accepted code: " + accepted_script_code))
        execute(accepted_script_code, true, script_name, catch_errors)
        ready = true
        ranScript()
    }

    function evaluate(expression, script=script_name) {
        if (!ready) return null;
        logger.trace(() => ("Evaluating expression: " + expression))
        return lua_engine.evaluate(expression, script, true, catch_errors)
    }

    function execute(statement, force=false, script=script_name) {
        if (!ready && !force) return;
        logger.trace(() => ("Executing statement: " + statement))
        lua_engine.execute(statement, script, true, catch_errors)
    }
}