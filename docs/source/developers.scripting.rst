Lua Scripting
-----------------
.. _lua_scripting:

Introduction
^^^^^^^^^^^^^^^^^^^^^^^^

**ShoopDaLoop** supports embedded **Lua scripts** for querying and controlling the application. For example, these are used to define how **ShoopDaLoop** reacts to control MIDI events.
Lua scripts can be provided by the user and don't require a re-installation of the software.

**Lua** inside **ShoopDaLoop** is sandboxed for security making a large part of the standard libary unavailable. Only a whitelisted list of functions can be used. See **sandbox.lua** for details. Most notably: not any module can be imported through **require**. Only **ShoopDaLoop**-provided modules can be used.

API and Libraries
^^^^^^^^^^^^^^^^^

The API consists of globally available functions and constants, in addition to functions and constants available through built-in libraries. Built-in libraries should be included in scripts using the `require` function. Check `src/lua/builtins/keyboard.lua` for an example.

Globally available APIs
"""""""""""""""""""""""

* **print(msg)**, **print_debug(msg)**, **print_error(msg)**, **print_info(msg)**: Print a message to the Frontend.LuaScript logger. Respective log levels are info (default), debug, error.

type: midi_control_port
"""""""""""""""""""""""

.. shoop_function_docstrings::
   src/python/shoopdaloop/lib/q_objects/MidiControlPort.py

module: shoop_control
"""""""""""""""""""""

Provides basic interfacing with **ShoopDaLoop**. Note that these functions are provided as bindings into the application - they are not written in Lua.

.. shoop_function_docstrings::
   src/python/shoopdaloop/lib/q_objects/ControlHandler.py

.. shoop_function_docstrings::
   src/python/shoopdaloop/lib/q_objects/ControlInterface.py


module: shoop_coords
""""""""""""""""""""

Provides helper functions to manipulate loop and track coordinates. Implemented in `shoop_coords.lua`.

.. shoop_function_docstrings::
   src/python/shoopdaloop/lib/lua/lib/shoop_coords.lua

module: shoop_helpers
"""""""""""""""""""""

Provides helper functions for advanced control. Implemented in `shoop_helpers.lua`.

.. shoop_function_docstrings::
   src/python/shoopdaloop/lib/lua/lib/shoop_helpers.lua

module: shoop_format
""""""""""""""""""""

Provides helper functions for formatting strings. Implemented in `shoop_format.lua`.

.. shoop_function_docstrings::
   src/python/shoopdaloop/lib/lua/lib/shoop_format.lua