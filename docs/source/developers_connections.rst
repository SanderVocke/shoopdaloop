States And Connections
------------------------

The basic principles of muting, monitoring, recording, playing back etc. are straightforward.
However, in the case of a dry/wet combined looping setup, these principles can get confusing and/or impossible to implement perfectly, given that only a single FX processor exists.


For example, it is not possible to re-record the dry audio of multiple loops into their respective wet audio channels if they are sharing the same track (and thus the same FX processor). Their wet audio would be combined.
Likewise, it is not possible to monitor the input while re-recording a loop.


Therefore some compromises have been made to select what is probably the most desirable wiring for each state of loop(s) and monitoring.


The following diagrams show the internal wiring of ShoopDaLoop's ports and loops, including which signal paths are disabled / silent in different track/loop states.

.. figure:: resources/connections_states.drawio.svg
   :width: 800px
   :alt: Connections in different states

   Connections in different states.
