Keyboard control
----------------

The computer keyboard can be used to control many aspects of **ShoopDaLoop**. Note that the keyboard control behavior is implemented as a custom **Lua script** (keyboard.lua),
which means that if so desired, you can make your own keyboard control modifications by duplicating the script and modifying it.

The help text of the default **keyboard.lua** is shown here for reference. In the native egui application, open **Settings** and select **Scripts** to view this help, edit startup enablement, restart the script, and inspect errors or logs. Keyboard presses are ignored while egui is accepting text input; key repeats are suppressed, and held sampler keys are released when the window loses focus. ``keyboard.lua`` is enabled by default on first run and its setting is preserved in the fresh egui settings document after **Save**.

.. shoop_lua_docstring::
   src/lua/builtins/keyboard.lua