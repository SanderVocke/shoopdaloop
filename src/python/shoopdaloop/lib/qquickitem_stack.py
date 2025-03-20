from PySide6.QtQml import QQmlEngine, QQmlContext
from PySide6.QtQuick import QQuickItem

def format_qml_stack_for_qquickitem(item):
    # Get the QQmlEngine
    context = QQmlEngine.contextForObject(item)
    if context:
        engine = context.engine()
        if engine:
            return f"Stack trace:\n\n{engine.evaluate('(new Error).stack').toString()}\n\n"
        else:
            return "No QQmlEngine found"
    else:
        return "No QQmlContext found"
