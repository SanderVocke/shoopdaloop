Lua scripting
-------------
.. _lua_scripting:

Runtime and ownership
~~~~~~~~~~~~~~~~~~~~~

ShoopDaLoop embeds pinned omniLua with Lua 5.4 semantics. Each script has an
isolated state owned by the application runtime. Stopping or restarting a script
removes its callbacks, timers, logical MIDI ports, connections, and queued
output.

The sandbox exposes selected standard-library functions and ShoopDaLoop modules.
It prevents ordinary module/file access but should still be treated as a
compatibility boundary for trusted local scripts, not as a hardened security
boundary.

Script management
~~~~~~~~~~~~~~~~~

Open **Settings → Scripts** to inspect lifecycle, errors, help, activity, logs,
and MIDI diagnostics. Native builds can add, reload, and remove user script
files. Browser builds manage bundled scripts and sources embedded in sessions,
without machine path actions.

``keyboard.lua`` is enabled on first run. The APC Mini script is available but
disabled by default. Persistent changes apply after **Save**; runtime Stop,
Restart, and Reload do not alter the settings draft. Source-bearing scripts in a
``.shoop`` session are syntax-checked before transactional session commit and
round-trip without machine paths.

MIDI rules
~~~~~~~~~~

Scripts create logical input/output ports with full-name regular expressions.
Discovery is hotplug-aware, queues are bounded, and positive output rates are
paced without catch-up bursts. Native services use JACK or midir. Browser
services use explicitly enabled Web MIDI. Per-rule endpoint and failure state is
published to the Scripts tab.

Built-in modules
~~~~~~~~~~~~~~~~

``shoop_control``
  Synchronous queries and typed mutations for loops, tracks, global controls,
  callbacks, timers, and logical MIDI ports. Stable ``Key_*``,
  ``KeyModifier_*``, loop-mode, event-type, and sentinel constants are exposed
  for bundled and user scripts.

``shoop_coords``

.. shoop_function_docstrings::
   src/lua/lib/shoop_coords.lua

``shoop_helpers``

.. shoop_function_docstrings::
   src/lua/lib/shoop_helpers.lua

``shoop_format``

.. shoop_function_docstrings::
   src/lua/lib/shoop_format.lua
