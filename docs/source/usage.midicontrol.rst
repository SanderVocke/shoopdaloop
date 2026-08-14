MIDI controllers
----------------

ShoopDaLoop controller integration is script-based. Open **Settings → Scripts**
to enable the bundled APC Mini script or manage other scripts. Scripts are
grouped by kind in a table; its icon controls open separate help, log, and status
windows showing callbacks, timers, logical MIDI rules, matched and connected
endpoints, queue drops, and failures.

Native builds discover MIDI through the selected JACK or midir service. Browser
builds discover physical endpoints after the independent **Enable Web MIDI +
SysEx** action. Denied or unavailable Web MIDI does not disable audio, keyboard
control, or the rest of the application.

Autoconnection
~~~~~~~~~~~~~~

A script-created logical MIDI port contains a direction and a full-name regular
expression. It connects to every compatible matching endpoint and reconnects
after hotplug. An empty expression matches nothing. Output rules can set a
positive message rate; zero is unthrottled. Queues are bounded and expose drop
counters rather than growing indefinitely.

Custom controllers
~~~~~~~~~~~~~~~~~~

Controller behavior can use the ``shoop_control`` Lua API, callbacks, timers,
and MIDI helpers. Native builds may add user script files. Browser builds use
bundled and session-contained sources and intentionally omit machine file-path
actions. See :ref:`Lua scripting <lua_scripting>`.
