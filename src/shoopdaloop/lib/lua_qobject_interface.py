from PySide6.QtCore import Qt, QObject, QMetaObject, Q_ARG, Q_RETURN_ARG, Signal, Property, Slot, QTimer, QMetaType
import json
from .logging import Logger

logger = None

def qt_typename(qt_type):
    name = QMetaType.Type(qt_type).name
    return qt_typename.lookup[name] if name in qt_typename.lookup.keys() else name
qt_typename.lookup = {
    'Bool': bool,
    'Int': int,
    'String': str,
    'Double': float,
} 

def lua_passthrough(val):
    return val

def as_int(lua_val):
    return int(lua_val)

def as_float(lua_val):
    return float(lua_val)

def as_str(lua_val):
    return str(lua_val)

def as_callable(lua_val):
    return lambda *args: lua_val(*args)

lua_int = [ int, lua_passthrough ]
lua_bool = [ bool, lua_passthrough ]
lua_str = [ str, lua_passthrough ]
lua_float = [ float, lua_passthrough ]
lua_callable = [ 'QVariant', as_callable ]

# TODO:
# Currently, all the Lua-exposed methods have their arguments stored in a QVariant list.
# This is because it is not possible to invoke these methods by name from PySide using
# invokeMethod (see https://bugreports.qt.io/browse/PYSIDE-2500).

# Creates a global object in the Lua runtime with the given name.
# All functions which are included in the qobject class' static "interface_names" member
# will be registered as members on the returned Lua object.
# Interface_names is a list of lists, where each sublist represents one callback in the form:
# [ callback_name, arg1_converter, arg2_converter, ... ]
# the arg converters should be functors which convert the given Lua argument back into Python
# types. For simple primitive arguments, "lua_passthrough" (identity functor) can be used.
# Constants can also be shared into Lua by populating the lua_constants list.
def create_lua_qobject_interface(lua_engine, qobject):
    global logger
    if logger == None:
        logger = Logger('Frontend.LuaQObjectInterface')
    
    logger.debug(lambda: "Creating Lua interface for QObject {}".format(qobject))
    
    module = lua_engine.evaluate('return {{}}')
    if_registrar = lua_engine.evaluate('return function (module, name, member) module[name] = member; return module end')
    const_registrar = lua_engine.evaluate('return function (module, name, value) module.constants = module.constants or {}; module["constants"][name] = value; return module end')
    
    meta_methods = dict()
    for i in range(qobject.metaObject().methodCount()):
        method = qobject.metaObject().method(i)
        meta_methods[str(method.name(), 'ascii')] = method

    # Scan for invokable functions.
    methods = dict()
    for i in range(qobject.metaObject().methodCount()):
        method = qobject.metaObject().method(i)
        methods[str(method.name(), 'ascii')] = method

    # Now register callbacks into the qobject functions.
    if hasattr(qobject, 'lua_interfaces'):
        for interface in type(qobject).lua_interfaces:
            if not interface[0] in methods.keys():
                continue
            method = methods[interface[0]]
            returntypename = qt_typename(method.returnType())
            def callback(interface, returntypename, lua_engine, *args):
                if returntypename == 'Void':
                    logger.trace(lambda: "Calling void QML method {} with args {}".format(interface, str(args)))
                    qobject.metaObject().invokeMethod(
                        qobject,
                        interface[0],
                        Qt.AutoConnection,
                        Q_ARG('QVariantList', [interface[idx+1][1](arg) for idx,arg in enumerate(args)]),
                        Q_ARG('QVariant', lua_engine)
                    )
                    return
                logger.trace(lambda: "Calling QML method {} with args {}".format(interface[0], str(args)))
                rval = qobject.metaObject().invokeMethod(
                    qobject,
                    interface[0],
                    Qt.AutoConnection,
                    Q_RETURN_ARG(returntypename),
                    Q_ARG('QVariantList', [interface[idx+1][1](arg) for idx,arg in enumerate(args)]),
                    Q_ARG('QVariant', lua_engine)
                )
                rval_converted = lua_engine.to_lua_val(rval)
                logger.trace(lambda: "Result of calling {} QML method {} with args {}: {} (LUA: {})".format(returntypename, interface[0], str(args), str(rval), str(rval_converted)))
                return rval_converted
            
            logger.debug('Registering Lua interface {} for {}, with return type {}'.format(interface[0], qobject, returntypename))
            module = if_registrar(module, interface[0],
                lambda *args, interface=interface, returntypename=returntypename, lua_engine=lua_engine: callback(interface, returntypename, lua_engine, *args)
            )
    
    if hasattr(qobject, 'lua_constants'):
        for constant in type(qobject).lua_constants:
            module = const_registrar(module, constant[0], constant[1])
    
    return module
