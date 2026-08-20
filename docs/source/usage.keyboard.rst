Keyboard control
----------------

The computer keyboard can control many aspects of **ShoopDaLoop**. Keyboard behavior is implemented by the bundled **Lua script** ``keyboard.lua``. Native builds can use a modified copy as a user script; browser builds fetch it from the packaged external built-ins tree or use session-contained sources.

The help text of the default **keyboard.lua** is shown here for reference. Open **Settings** and select **Scripts** to view this help, edit startup enablement, restart the script, and inspect errors or logs. Keyboard presses are ignored while the GUI is accepting text input; key repeats are suppressed, and held sampler keys are released when the window loses focus. Newly discovered scripts are disabled until enabled in the dynamic identity list and saved.

.. shoop_lua_docstring::
   resources/builtins/keyboard.lua