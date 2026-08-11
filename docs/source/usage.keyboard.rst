Keyboard control
----------------

The computer keyboard can control many aspects of **ShoopDaLoop**. Keyboard behavior is implemented by the bundled **Lua script** ``keyboard.lua``. Native builds can use a modified copy as a user script; browser builds use bundled or session-contained sources.

The help text of the default **keyboard.lua** is shown here for reference. Open **Settings** and select **Scripts** to view this help, edit startup enablement, restart the script, and inspect errors or logs. Keyboard presses are ignored while egui is accepting text input; key repeats are suppressed, and held sampler keys are released when the window loses focus. ``keyboard.lua`` is enabled by default on first run and its setting is preserved in the fresh egui settings document after **Save**.

.. shoop_lua_docstring::
   src/lua/builtins/keyboard.lua