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
* **Recording Dry Into Wet**: Only available for dry+wet tracks. The loop is playing the dry recording through the FX/synth into the wet, which is being recorded. This way, changed synth/FX settings can be baked into the wet recording such that normal (wet) playback can again be used.

Loops can be transitioned by hovering over them with the mouse. Hovering over certain buttons will open a dropdown with more options. For example, playing dry through wet appears when hovering the play button.

Normally, a loop will keep recording until transitioned to another state. However, it is also possible to record for a fixed amount of master loop cycles so that recording does not need to be manually stopped.


Selecting and Targeting
^^^^^^^^^^^^^^^^^^^^^^^^^

Loops can be **selected** (yellow border) by clicking their icon next to the buttons on the left-hand side. Selection is useful for transitioning multiple loops together. Performing a transition on any loop will also perform the same transition on all currently selected loops.

A single loop can be **targeted** (orange border) by double-clicking it. The behavior of certain loop transitions is different if another loop is currently targeted. For example, the "record N / record 1" button is replaced by a **record synced** button. This button will ensure that recording starts when the targeted loop restarts, and that the recording duration will match the targeted loop.


Pre-recording
^^^^^^^^^^^^^^^

Oftentimes, a catchy hook or riff will start before the "1" of the music. Or, the loop starts on 1 but you want to start it will e.g. a small fill the first time. This makes it complicated to loop sometimes, because you would need to anticipate one master loop cycle earlier than the actual looping part starts.

For this reason, loops in **ShoopDaLoop** are already **pre-recording** in the cycle before their real recordings starts. You normally won't notice this because the data for this part is stored but usually never played.

To hear the pre-recorded part back, you need to enable **pre-playback**. This is done in the loop details window (opened from the loop context menu when right-clicking it).

TODO: describe in detail with pictures

