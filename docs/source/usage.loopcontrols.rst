Controlling Loops
-----------------

Synchronization On/Off
^^^^^^^^^^^^^^^^^^^^^^^^

Loop mode transitions in **ShoopDaLoop** happen synchronized to the sync loop. However, immediate actions can also be done if needed. To do so, turn *synchronization off* when performing the action. Note that this does not affect already queued actions or playing loops - only the actions done while turned off.

The *synchronization active* button at the top of the screen shows the current state of synchronization. A timer symbol (SYMBOL) means synchroniation on, an exclamation point (SYMBOL) means synchronization off.

There are two ways to control this:

* Click the button to toggle between the two modes;
* Synchronization is always off while a Ctrl key is held (usually easier).


Always-on Recording
^^^^^^^^^^^^^^^^^^^

Having to manually trigger recordings ahead of time can cost focus and break the flow.
For that reason, **ShoopDaLoop** to record a loop in hindsight. You can just play, and when something nice happens, capture it afterward.

The feature doesn't need to be manually enabled. All you need to do is press the "grab" button or respective keyboard/MIDI control.

Note that "grabbing" will grab **n cycles** amount of sync loop cycles, up to and including the last finished cycle. In most cases you will want to grab just after the sync loop has finished of the portion you want to grab.

TODO: grab button


Loop Modes and Transitions
^^^^^^^^^^^^^^^^^^^^^^^^^^^

Loops can be in the following modes:

* **Stopped**: The loop is not playing. This is the initial state of a loop.
* **Empty**: Stopped and also no data stored in the loop.
* **Recording**: The loop is recording. For dry+wet tracks, both dry and wet are recording at the same time.
* **Playing**: The loop is playing. For dry+wet tracks, only the wet recording is being played back such that the FX/synth processing may be disabled.
* **Playing Dry Through Wet**: Only available for dry+wet tracks. The loop plays back the dry recording through FX/synth being processed in real-time. This way, e.g. FX/synth settings can be changed and the result is immediatlely audible.
* **Recording Dry Into Wet**: Only available for dry+wet tracks. The loop is playing the dry recording through the FX/synth into the wet, which is being recorded. This way, changed synth/FX settings can be baked into the wet recording such that normal (wet) playback can again be used.

Using a long-press on the **record one cycle** button, you can also choose to **record N cycles** of the sync loop.

Loops can be transitioned by hovering over them with the mouse. There are also keyboard controls (currently only documented in **keyboard.lua**). Hovering over certain buttons will open a dropdown with more options. For example, playing dry through wet appears when hovering the play button.

Normally, a loop will keep recording until transitioned to another state. However, it is also possible to record for a fixed amount of sync loop cycles so that recording does not need to be manually stopped.


Selecting and Targeting
^^^^^^^^^^^^^^^^^^^^^^^^^

Loops can be **selected** (yellow border) by clicking their icon next to the buttons on the left-hand side. Selection is useful for transitioning multiple loops together. Performing a transition on any loop will also perform the same transition on all currently selected loops.

A single loop can be **targeted** (orange border) by double-clicking it. The behavior of certain loop transitions is different if another loop is currently targeted. For example, the "record N / record 1" button is replaced by a **record synced** button. This button will ensure that recording starts when the targeted loop restarts, and that the recording duration will match the targeted loop.


Pre-recording
^^^^^^^^^^^^^^^

Oftentimes, a catchy hook or riff will start before the "1" of the music. Or, the loop starts on 1 but you want to start it will e.g. a small fill the first time. This makes it complicated to loop sometimes, because you would need to anticipate one sync loop cycle earlier than the actual looping part starts.

For this reason, loops in **ShoopDaLoop** are already **pre-recording** in the cycle before their real recordings starts. You normally won't notice this because the data for this part is stored but usually never played.

To hear the pre-recorded part back, you need to enable **pre-playback**. This is done in the loop details window (opened from the loop context menu when right-clicking it).

..
  TODO: describe in detail with pictures

MIDI looping
^^^^^^^^^^^^

In principle, MIDI loops work the same as audio loops. However, playing back a MIDI signal will not always result in the exact same sound as the first recording, because:

* The audio synthesis (in plugin or external JACK application) may have internal state that is not directly controlled by MIDI;
* MIDI has a state, which includes all CC values, pitch bend, notes already active at recording start, etc.

The way ShoopDaLoop approaches MIDI playback is to approximate the state at the start of recording as closely as possible. That means:

* ShoopDaLoop will restore states like CCs (including sustain pedal, mod wheel, pitch) to the state they were in when recording started, at the start of every playback loop.
* If a note was already active when recording started, ShoopDaLoop will remember this and play the same note at the start of every playback loop. One advantage of this is if a note was played just slightly before recording start, it will sound indistinguishable in most cases. Note that this does not in include notes that are finished (on + off) just before recording start.

Composite Loops
^^^^^^^^^^^^^^^

A **composite loop** can be created by selecting an empty slot, then holding **Alt** and clicking another loop. The other loop is added to the composite loop composition:

* Normally at the end of the current sequence. Note that the same loop may also be clicked multiple times to add it repeatedly.
* If **Ctrl** is also held, it is added in parallel of the current sequence.

Note that **Alt**+click will append to the first "timeline". So for example, if a short loop is composed in parallel with a long one, **Alt**-click will add an additional loop to play right after the short one.

For advanced editing of the sequence, the loop details window should be used (note that at the time of writing this, that is unimplemented).

Composite loops are shown in pink; if a composite loop is (solely) selected, all its sub-loops are highlighted with a pink border.

..
    TODO: pictures

Playback
""""""""

Playing back a composite loop will play the loops as sequenced. Empty sub-loops are skipped. The psrogress indicator on the composite loop shows the total progress. The playback will cycle back around to the start of the sequence.


Recording
"""""""""

In order to record a composite loop, the sub-loops must already have contents so their lengths can be determined. That means you will first need to record the subloops separately or manually set their lenghts.

When this is the case, pressing "record" on the composite loop will re-record the subloops in sequence.

Note that there is a special case if the same subloop is sequenced multiple times. It will not re-record multiple times. Instead, after re-recording it the first time, additional occurrences in the sequence are skipped with the subloop idle.