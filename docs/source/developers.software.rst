Software Design
----------------

Architecture
^^^^^^^^^^^^^

**ShoopDaLoop** is built from a Rust engine, Rust/cxx-qt frontend glue, QML UI, and Lua scripting.

.. uml::
    :caption: Overall software stack

    component engine [
        shoop_engine (Rust engine)
    ]
    component frontend [
        shoopdaloop (QML front-end)
    ]
    collections extensions [
        Front-end Extensions (Rust + cxx-qt)
    ]
    component scripting [
        LUA scripts
    ]

    extensions ..> engine : uses
    frontend - extensions
    frontend ..> scripting : embeds

The front-end prepares configuration and presents observations, while timing-authoritative state machines run in the engine. In particular, composite playlists are compiled off the audio thread into immutable plans and accepted through bounded commands; Qt signals and snapshot polling do not advance them.

Loop content and logical length changes are non-topological. The control side validates and prepares replacement storage, one bounded command commits all affected channels at a callback boundary, and displaced storage is reclaimed off the realtime thread. These operations preserve the backend session and audio driver and do not rebuild the graph; whole-session replacement remains reserved for loading sessions and switching drivers.

The **shoop_engine** crate handles:

* All real-time audio + MIDI processing
* Interconnections of ports, loop channels and FX
* JACK, CPAL and MIDI driver integration
* Logging and profiling
* Basic and composite loop synchronization, sample-boundary event resolution, and nested propagation
* Bounded composite command, plan-reclamation, fault, trace, and snapshot transport

The **front-end + extensions** handle:

* The user interface
* Session saving/loading
* Composite authoring, validation feedback, and session persistence
* Translation of UI references into stable engine identities and prepared plans
* Thread-decoupled command submission and observational snapshot display

The **LUA scripts** are meant for parts that may need to be added/modified by individual users, such as:

* MIDI controller profiles
* Keyboard control

Carla subprocess architecture
^^^^^^^^^^^^^^^^^^^^^^^^^^^^^

Carla hosting is separated into a frontend-independent processor contract, a versioned control protocol, process supervision, and realtime block transport. The installed ShoopDaLoop executable starts itself in a hidden worker mode before constructing Qt. Each worker authenticates a loopback control connection with a random nonce and maps a generation-specific temporary block area.

Control messages carry chain, request, protocol, and process-generation identity. Carla LV2 creation, state access, and external-UI hosting remain in the worker. Audio and MIDI use three ownership-tracked shared-memory slots with fixed channel, frame, event, and byte bounds. A parent deadline can abandon a slot without reusing memory that a late worker may still access; stale generations cannot publish into a replacement mapping. Control, state, logs, launch, and process destruction remain outside block transport.

The session owns a unique ``CarlaRealtimeProcessor`` endpoint. It contains only preallocated buffers, shared-memory state, atomics, and a pre-created bridge-thread wake handle; it does not share an ordinary mutex or control socket with the UI. A bounded command channel and atomic snapshot connect frontend operations to the non-realtime bridge owner. The local bridge uses ``Thread::unpark`` notification. A subprocess bridge wakes its worker with a nonce-derived fixed-size loopback UDP datagram; only notification identity crosses that socket, never audio or MIDI payloads. Tracy zones cover bridge submission, notification, waiting, completion/fallback, and worker plugin processing.

The parent supervisor retains the last confirmed state and desired activity independently from the child. The bounded checkpoint policy refreshes only after a complete explicit save or successful restore; loading a session restores its state before recovery is offered, and failed or partial operations leave the previous checkpoint intact. It also drains stdout and stderr into separate fixed-capacity generation records. A restart creates a new mapping and process generation before restoring state and activity. The direct host implements the same high-level processor contract and remains the compatibility path when isolation is disabled.

The protocol and settings crates contain no Qt or egui dependency. Frontends adapt published lifecycle, generation, diagnostics, and recovery operations rather than implementing process or transport behavior themselves. ``CARLA_SUBPROCESS_BENCHMARK.md`` records the benchmark contract, Linux percentile/deadline/CPU results, native Windows/Linux/macOS direct-versus-subprocess matrices, and the exact release commands used for transport tuning.

Build And Packaging
^^^^^^^^^^^^^^^^^^^^

The combination of different languages, OSes and the dual dependency on Qt and PySide has resulted in a complex build approach. Cargo drives the Rust build and package helper binaries, while build scripts integrate Qt/cxx-qt and other native libraries.

For the official release artifacts, the setup is more complicated because we need to be binary-compatible with the Qt libraries that ship with PySide. Documentation for this will be added in the future, when the still pending improvements to this build flow are finished.


Debugging
^^^^^^^^^^

There are several tools at your disposal for debugging:

* The **logging framework** is available at all levels in the software stack. It allows for logging at different levels, and filtering on levels or components where the message originated from.
* The built-in **profiler** allows checking which parts take up the most time in the audio process loop. It can be accessed from the user interface.
* The built-in **debug inspector** can inspect engine objects' states from the user interface.
* ShoopDaLoop can be run with the `-d PORT` flag to connect a QML debug client or profiler (such as those offered from QtCreator).


Testing
^^^^^^^^

The test suites for **ShoopDaLoop** are by no means complete, but do test essential functions at several levels. The following testing tools exist:

* Rust unit and integration tests powered by **cargo**.
* QML unit and integration tests powered by **Qt Quick Test**.

The QML integration tests come closest to "system-level". For example, there are tests there which can check cycle-accurately that the correct audio samples are produced based on what the user clicked in the user interface.

Coverage is generated for Rust and QML where enabled. QML coverage generation is powered by `qoverage <https://github.com/SanderVocke/qoverage>`_.



Continuous integration
^^^^^^^^^^^^^^^^^^^^^^^

CI automation code is in-repo for **GitHub Actions**.
