Lua Scripting
-----------------

Introduction
^^^^^^^^^^^^^^^^^^^^^^^^

**ShoopDaLoop** supports embedded **Lua scripts** for querying and controlling the application. For example, these are used to define how **ShoopDaLoop** reacts to control MIDI events.
Lua scripts can be provided by the user and don't require a re-installation of the software.



Lua Environment
^^^^^^^^^^^^^^^^^^^^^^^^

The following things should be kept in mind when writing **Lua** snippets or scripts for **ShoopDaLoop**.

Globals and context variables
""""""""""""""""""""""""""""""

* **Lua**'s behavior regarding global declarations is slightly altered within **ShoopDaLoop** scripts:

  * Globals should be declared with:
  
    * `declare_global("myvar", "myinitvalue")` (ignored if the variable already exists), or

    * `declare_new_global("myvar", "myinitvalue")` (error if the variable already exists)

  * Attempting to declare a global by a simple assignent statement (e.g. `x = 3`) will fail. This prevents accidents with typos.
  
  * Attempting to reference (read) a non-existing global will also fail.

  * Global state is preserved between script calls.

  * After a global is declared, it can be modified or read with direct assignment / reference, no special syntax or functions needed.

  * In most cases, globals are a bad idea. Use context variables (see below) instead. Otherwise, e.g. your MIDI controller handlers for different controllers can see each other's global variables.

* **Contexts** are introduced. These act just like globals, except they are valid only for scripts/commands called with the same **context**.
  These are useful for creating pseudo-global state, which will be invisible to scripts run in different contexts.

  * Instead of `declare_<>new_>global("a", "b")`, `declare_<new_>in_context("a", "b")` is used for context state.

  * All other rules match those of globals (can be used normally after declaration).

  * Context variables are persisted between calls to the same script. You can see them as similar to "static variables" in C.
  
  * If you write scripts, you don't need to manage contexts directly. **ShoopDaLoop** will ensure that:

    * Scripts associated with the same MIDI controller profile share a context

    * ...


API
^^^

.. shoop_function_docstrings::
   src/shoopdaloop/lib/q_objects/ControlHandler.py ControlHandler

* **print(msg)**, **print_debug(msg)**, **print_error(msg)**, **print_info(msg)**: Print a message to the Frontend.LuaScript logger. Respective log levels are info (default), debug, error.
* For the rest of the API, there are plans to auto-generate documentation, but for now the available interfaces can be found in the source files `ControlHandler.py` and `ControlInterface.py`. Look for functions with a docstring. These functios are exposed in the LUA environment as shoop.<function_name>.
* Check `lib/lua/builtins/keyboard.lua` for an example.