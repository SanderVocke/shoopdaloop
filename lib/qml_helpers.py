from PyQt6.QtQml import qmlRegisterType

def register_qml_class(t, name):
    qmlRegisterType(t, name, 1, 0, name)

