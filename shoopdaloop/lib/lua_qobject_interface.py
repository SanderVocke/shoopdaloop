def lua_passthrough(val):
    return val

# Creates a global object in the Lua runtime with the given name.
# All functions which are included in the qobject class' static "interface_names" member
# will be registered in the global Lua object, and set up such that they call back to
# the qobject when called.
# Interface_names is a list of lists, where each sublist represents one callback in the form:
# [ callback_name, arg1_converter, arg2_converter, ... ]
# the arg converters should be functors which convert the given Lua argument back into Python
# types. For simple primitive arguments, "lua_passthrough" (identity functor) can be used.
def create_lua_qobject_interface(lua_module_name, scripting_engine, qobject):
    # First create the global "module".
    scripting_engine.execute(
'''declare_new_global("{0}", {{}})

declare_new_global("__{0}_register_cb", function (name, cb)
    {0}[name] = cb
end)
'''.format(lua_module_name))

    # Now register callbacks into the qobject functions.
    if hasattr(qobject, 'lua_interfaces'):
        registrar = scripting_engine.eval('function (name, cb) __{}_register_cb(name, cb) end'.format(lua_module_name))
        for interface in type(qobject).lua_interfaces:
            # The callback will call back into Python via converter classes for each argument.
            bound_member = getattr(qobject, interface[0])
            registrar(interface[0], 
                lambda *args, member=bound_member, itf=interface: member(       # call the member function...
                    *[itf[idx+1](arg) for idx,arg in enumerate(args)] # ...while going through the converters.
                )
            )
