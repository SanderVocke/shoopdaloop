.. _composite_loops:

Composite Loops
----------------

A **composite loop** is a virtual loop that doesn't contain any audio or MIDI data directly. Instead, it controls other loops (which may themselves also be composite loops). There are two types of composite loops:

**Regular composite loops** behave like a normal loop, but contain a sequence of triggers inside. They can be used to combine multiple small loops into larger ones. They can be used as **scenes** (trigger multiple other loops in parallel) or **sequences** (trigger a sequence of other loops). They behave as follows:

* Triggering a **regular composite loop** will make it behave like a normal loop. For example, pressing **record** on one will record the planned loops as defined in the sequence. Likewise for playback, etc.
* **Regular composite loops** loop around. That is to say, when they reach the end, they restart.

**Script composite loops** are slightly different. In a **script composite loop**, you specify the mode for each scheduled trigger. Thus, a **script composite loop** may first record a loop and then play it, while starting recording on another. This makes them suitable for pre-planning a scripted looping session. Details:

* A **script composite loop** has different buttons on it than a normal loop or regular composite. It can only be started or stopped.
* A **script composite loop** will not loop around, but rather just stop at the end.

All kinds of **composite loops** can be hierarchically combined.

Creating Composite Loops
^^^^^^^^^^^^^^^^^^^^^^^^

Right-click any loop slot and click **Create composite**. The loop changes color. Clearing the loop reverts it back to non-composite.

To edit the contents of a **composite loop**, open the **details** pane at the bottom and select the loop by clicking on the empty state indicator on the loop. Loops can be dragged into the composite loop's timeline.

There is also a drop-down box in the details pane to toggle between **regular** and **script**.

There is also a quicker way to create composites:

* *Alt+Click* while a loop is selected will add the clicked loop to the end of the selected composite loop (or create one if empty).
* *Alt+Ctrl+Click* while a loop is selected will add the clicked loop in parallel at the start of the selected composite loop (or create one if empty).