Lua scripting
-------------
.. _lua_scripting:

Runtime and ownership
~~~~~~~~~~~~~~~~~~~~~

ShoopDaLoop embeds pinned omniLua with Lua 5.4 semantics. Native and browser
builds run the same bundled libraries, ``keyboard.lua``, and APC Mini script.
Each script has an isolated state owned by the application runtime. Stopping or
restarting a script removes its callbacks, timers, logical MIDI ports,
connections, queued output, and script-owned dialogs.

Every script must make ``shoop_announce_api_version(major, minor)`` its first
Shoop API call. The current version is ``1.3``. A script runs only when its major
equals the host major and its minor is no newer than the host minor. Missing,
malformed, repeated, or incompatible announcements cancel initial execution
before versioned side effects. The global two-integer signature is independent
of modules and reserved to remain stable across future API versions. See
``docs/lua_dialog_api.md`` for the compatibility and migration contract.
Lua API compatibility is independent of the ``.shoop`` session format version.
An incompatible source remains inspectable and exportable, but cannot be
started.

The sandbox exposes selected standard-library functions and ShoopDaLoop modules.
It prevents ordinary module access and restricts file access to paths below each
script, but should still be treated as a compatibility boundary for trusted local scripts, not as a hardened security
boundary.

Script management
~~~~~~~~~~~~~~~~~

Open **Settings → Scripts** to inspect lifecycle, errors, help, activity, logs,
and MIDI diagnostics. Native builds can add, reload, and remove user script
files. Browser builds manage bundled scripts and sources embedded in sessions,
without machine path actions. Both targets can load a UTF-8 ``.lua`` file from
the run-once picker or by OS drag and drop after confirmation. Run-once sources
remain restartable in memory, are independent of session replacement and
serialization, and disappear when the app closes. Loading a same-named version
stops the active version and retains both entries under unique display names.
Every listed script can be exported as its exact ``.lua`` source. A built-in,
example, user, or run-once script can be included in the session; this transfers
the current source to session ownership. A session script can instead be
converted to run once or removed from the session.

``keyboard.lua`` is enabled on first run. The APC Mini script is available but
disabled by default. Persistent changes apply after **Save**; runtime Stop,
Restart, and Reload do not alter the settings draft. Source-bearing scripts in a
``.shoop`` session are syntax-checked before transactional session commit and
round-trip without machine paths.

Browser builds target ``wasm32-unknown-unknown`` and run the same pure-Rust
omniLua scripting manager cooperatively. Version checks, script-owned dialogs,
keyboard callbacks, session scripts, and permission-gated Web MIDI control use
the shared cross-target contracts.

MIDI rules
~~~~~~~~~~

Scripts create logical input/output ports with full-name regular expressions.
Discovery is hotplug-aware, queues are bounded, and positive output rates are
paced without catch-up bursts. Native services use JACK or midir. Browser
services use explicitly enabled Web MIDI. Per-rule endpoint and failure state is
published to the Scripts tab.

Global APIs
~~~~~~~~~~~

``shoop_announce_api_version(major, minor)``
  Mandatory first Shoop API call. Announces the non-negative integer major and
  minor version for which the script was designed.

``print(msg)``, ``print_debug(msg)``, ``print_error(msg)``, ``print_info(msg)``
  Add a message at the corresponding level to the script log.

Built-in modules
~~~~~~~~~~~~~~~~

``shoop_dialog``
  Script-owned simple and paged dialogs. Contents are ordered portable rich text
  and labeled buttons, and buttons may retain script callbacks. Scripts may
  request opening at startup or from callbacks; users retain window visibility
  and current-page control. Dialogs are destroyed with their owning runtime.
  See ``docs/lua_dialog_api.md`` for constructors, style fields, examples,
  errors, and lifecycle behavior.

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
