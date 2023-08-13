Controlling Loops
-----------------

Synchronization On/Off
^^^^^^^^^^^^^^^^^^^^^^^^

Loop mode transitions in **ShoopDaLoop** happen synchronized to the master loop. However, immediate actions can also be done if needed. To do so, turn *synchronization off* when performing the action. Note that this does not affect already queued actions or playing loops - only the actions done while turned off.

The *synchronization active* button at the top of the screen shows the current state of synchronization. A timer symbol (SYMBOL) means synchroniation on, an exclamation point (SYMBOL) means synchronization off.

There are two ways to control this:

* Click the button to toggle between the two modes;
* Synchronization is always off while a Ctrl key is held (usually easier).


Loop Modes and Transitions
^^^^^^^^^^^^^^^^^^^^^^^^^^^

Loops can be in the following modes:

* **Stopped**: The loop is not playing. This is the initial state of a loop.
* **Empty**: Stopped and also no data stored in the loop.
* **Recording**: The loop is recording. For dry+wet tracks, both dry and wet are recording at the same time.
* **Playing**: The loop is playing. For dry+wet tracks, only the wet recording is being played back such that the FX/synth processing may be disabled.
* **Playing Dry Through Wet**: Only available for dry+wet tracks. The loop plays back the dry recording through FX/synth being processed in real-time. This way, e.g. FX/synth settings can be changed and the result is immediatlely audible.
* **Recroding Dry Into Wet**: Only available for dry+wet tracks. The loop is playing the dry recording through the FX/synth into the wet, which is being recorded. This way, changed synth/FX settings can be baked into the wet recording such that normal (wet) playback can again be used.


Selecting and Targeting
^^^^^^^^^^^^^^^^^^^^^^^^^


