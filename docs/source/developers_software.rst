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