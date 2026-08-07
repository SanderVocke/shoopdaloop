Lua Scripting
-----------------
.. _lua_scripting:

Introduction
^^^^^^^^^^^^^^^^^^^^^^^^

**ShoopDaLoop** supports embedded **Lua scripts** for querying and controlling the application. For example, these are used to define how **ShoopDaLoop** reacts to control MIDI events.
Lua scripts can be provided by the user and don't require a re-installation of the software. Native egui builds run the same bundled libraries, ``keyboard.lua``, and APC Mini script as the retained QML frontend. Each script has an isolated Lua state owned by the application actor; stopping or restarting it removes its callbacks, timers, MIDI rules, connections, and queued output.

**Lua** inside **ShoopDaLoop** is sandboxed for compatibility and to keep scripts isolated, making a large part of the standard library unavailable. Only a whitelisted list of functions can be used. See **sandbox.lua** for details. Most notably, arbitrary modules cannot be imported through **require**; only **ShoopDaLoop**-provided modules can be used. Scripts should nevertheless be treated as trusted local code rather than as a hardened security boundary.

Native egui script management
^^^^^^^^^^^^^^^^^^^^^^^^^^^^^

Open **Scripts** in the main controls to inspect lifecycle state, errors, help text, callback/timer activity, logs, and MIDI diagnostics. The manager can enable, stop, restart, add, reload, and remove user scripts. Machine-wide bundled and path-based enablement is stored in ``script_settings.1``. Only ``keyboard.lua`` is enabled on first run. Source-bearing scripts inside a ``.shoop`` session are syntax-checked before session commit and are saved back with that session; machine paths are never embedded implicitly.

Native MIDI autoconnect uses an anchored full-name regular expression. Logical inputs connect to matching external outputs, and logical outputs connect to matching external inputs. Discovery is hotplug-aware, queues are bounded, and positive output-rate limits are enforced per logical output. Connection, send, regex, queue-drop, and callback failures remain visible in script status. Stopping a script closes everything it owns.

Browser builds target ``wasm32-unknown-unknown`` and intentionally do not link ``mlua``, native MIDI, or ``shoop_scripting``. The script manager reports scripting as unavailable, and script-bearing sessions are capability-rejected rather than partially executed.

API and Libraries
^^^^^^^^^^^^^^^^^

The API consists of globally available functions and constants, in addition to functions and constants available through built-in libraries. Built-in libraries should be included in scripts using the `require` function. Check `src/lua/builtins/keyboard.lua` for an example.

Globally available APIs
"""""""""""""""""""""""

* **print(msg)**, **print_debug(msg)**, **print_error(msg)**, **print_info(msg)**: Print a message to the Frontend.LuaScript logger. Respective log levels are info (default), debug, error.

module: shoop_control
"""""""""""""""""""""

Provides basic interfacing with **ShoopDaLoop**. Note that these functions are provided as bindings into the application - they are not written in Lua.

.. shoop_function_docstrings::
   src/rust/frontend/src/cxx_qt_shoop/rust/qobj_session_control_handler.rs

module: shoop_coords
""""""""""""""""""""

Provides helper functions to manipulate loop and track coordinates. Implemented in `shoop_coords.lua`.

.. shoop_function_docstrings::
   src/lua/lib/shoop_coords.lua

module: shoop_helpers
"""""""""""""""""""""

Provides helper functions for advanced control. Implemented in `shoop_helpers.lua`.

.. shoop_function_docstrings::
   src/lua/lib/shoop_helpers.lua

module: shoop_format
""""""""""""""""""""

Provides helper functions for formatting strings. Implemented in `shoop_format.lua`.

.. shoop_function_docstrings::
   src/lua/lib/shoop_format.lua