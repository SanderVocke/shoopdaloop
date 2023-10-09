import QtQuick 6.3
import ShoopDaLoop.PythonLogger

Item {
    id: root

    readonly property PythonLogger logger : PythonLogger { name: "Frontend.Qml.LuaUserScript" }

    property var scripting_context: null
    property var accepted_script_code : null
    property var script_code : null
    property string script_name : '<unknown script>'

    property bool ready : false

    onScript_codeChanged: {
        if (accepted_script_code !== null) {
            throw new Error("Script code cannot be changed")
        }
        accepted_script_code = script_code
    }

    onAccepted_script_codeChanged: {
        scripting_context = scripting_engine.new_context()
        logger.debug("Executing script in new context: " + scripting_context)
        logger.trace("Code: " + accepted_script_code)
        execute(accepted_script_code, true, script_name)
        ready = true
    }

    function evaluate(expression, script=script_name) {
        if (!ready) return null;
        logger.trace("Evaluating expression: " + expression + " in context: " + scripting_context)
        return scripting_engine.eval(expression, scripting_context, script, true)
    }

    function execute(statement, force=false, script=script_name) {
        if (!ready && !force) return;
        logger.trace("Executing statement: " + statement + " in context: " + scripting_context)
        scripting_engine.execute(statement, scripting_context, script, true)
    }
}