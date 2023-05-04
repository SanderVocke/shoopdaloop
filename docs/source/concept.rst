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

**ShoopDaLoop**'s loops are divided over **tracks**. Loops in the same **track** share their input/output port connections, volume/balance and effects/synthesis. Therefore, typically a track per instrument/part is used.



Effects / Synthesis
---------------------

**ShoopDaLoop** supports two track port connection modes: **direct** and **dry/wet**.

In **direct** mode, there is simply an input and an output.

In **dry/wet** mode, an effects and/or synthesis chain can be inserted for the track. When recording loops, the dry and wet signals are simultaneously recorded. This enables tricks such as re-playing the dry loop through live effects, playing back the wet while disabling the effects for CPU savings and re-synthesizing with different virtual instruments.

**Dry/wet** mode can be configured in two ways: using external JACK **send** and **return** ports or hosting plugins directly inside **ShoopDaLoop** via **Carla**. The latter has the advantage that the chain can be saved with the session and the chain setting can be remembered with individual loops.




Scripting
-----------

