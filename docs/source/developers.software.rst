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
