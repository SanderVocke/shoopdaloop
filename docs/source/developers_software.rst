Software Design
----------------

Architecture
^^^^^^^^^^^^^

**ShoopDaLoop** is built up as a back-end and a front-end, which are connected through a C API interface.

.. uml::
    :caption: Overall software stack

    component backend [
        libshoopdaloop (C++ back-end)
    ]
    component frontend [
        shoopdaloop (QML front-end)
    ]
    collections extensions [
        Front-end Extensions (Python + PySide6)
    ]
    interface interface [
        libshoopdaloop C API
    ]
    component scripting [
        LUA scripts
    ]

    backend - interface
    extensions ..> interface : uses
    frontend - extensions
    frontend ..> scripting : embeds

The split between front-end and back-end is not entirely pure, as different parts of the functionality are implemented in the layer where it is most convenient.

The **libshoopdaloop backend** handles:

* All real-time audio + MIDI processing
* Interconnections of ports, loop channels and FX
* Nearly all calls to the JACK API (exceptions below)
* Logging and profiling
* Basic loop synchronization (loop transitions)

The **front-end + Python extensions** handle:

* The user interface
* Session saving/loading
* Advanced loop synchronization (scheduling loop transitions over multiple master loop cycles)
* MIDI handling for MIDI controllers (non-cycle-accurate)

The **LUA scripts** are meant for parts that may need to be added/modified by individual users, such as:

* MIDI controller profiles


Build And Packaging
^^^^^^^^^^^^^^^^^^^^

The combination of different languages has resulted in a slightly complex build approach.
As the project is packaged as a Python package, an approach based on **pyproject.toml** has been taken.
For the C++ parts, **CMake** is used.
For combining the two, a tool called **py-build-cmake** is used.
The **CMake** part cannot be run trivially without the **py-build-cmake** integration because there is also some code generation taking place which requires both sides of the equation.
A source package cannot be built - only a wheel directly. Please refer to the build instructions for details.


Debugging
^^^^^^^^^^

There are several tools at your disposal for debugging:

* The **logging framework** is available at all levels in the software stack. It allows for logging at different levels, and filtering on levels or components where the message originated from. Note that in a release build, the **debug** and **trace** levels are removed from C++ during compilation, so less logging is available.
* The built-in **profiler** allows checking which parts take up the most time in the audio process loop. It can be accessed from the user interface.
* The built-in **debug inspector** can inspect back-end objects' states from the user interface.


Testing
^^^^^^^^

The test suites for **ShoopDaLoop** are by no means complete, but do test essential functions at several levels. The following testing tools exist:

* C++ unit and integration tests powered by **boost_ext::ut**.
* Python unit and integration tests powered by **pytest**, testing individual front-end extensions.
* QML unit and integration tests powered by **Qt Quick Test**.

The QML integration tests come closest to "system-level". For example, there are tests there which can check cycle-accurately that the correct audio samples are produced based on what the user clicked in the user interface.

Coverage is generated at each of the aforementioned test levels. QML coverage generation is powered by `qoverage <https://github.com/SanderVocke/qoverage>`_.



Continuous integration
^^^^^^^^^^^^^^^^^^^^^^^

CI automation code is in-repo for **GitHub Actions**.
