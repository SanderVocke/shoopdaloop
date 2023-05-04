Concept
=======================================

Master Loop
------------

In **ShoopDaLoop**, the **master loop** plays an important role in looping. Any new project starts with an empty **master loop**.

Actions on **loops** are synchronized to **triggers** of the **master loop**. A **trigger** is emitted when the **master loop** restarts. Examples:

* A requested **transition** (e.g. to recording, playing or stopped mode) will *usually* happen on the **master loop**'s next **trigger**.
* When a loop finishes playing, it will restart on the next **trigger** (which is usually instantly, as loops are typically multiples of the **master loop**'s length).

The master loop may itself hold audio and/or MIDI data. A typical use is a click track. However, it is also perfectly fine to leave it empty and use it for synchronization only.



Tracks
-------

**ShoopDaLoop**'s loops are divided over **tracks**. Loops in the same **track** share their input/output port connections and effects/synthesis. Therefore, typically a track per instrument/part is used.