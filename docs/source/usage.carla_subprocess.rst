Carla Process Isolation
-----------------------

Carla Rack and Patchbay FX chains can run either inside ShoopDaLoop or in a separate worker process for each chain. Select the mode under **Settings > Carla hosting**.

The setting is global and takes effect the next time ShoopDaLoop starts. Running FX chains are not migrated when the setting changes. Existing settings files default to **In application process**, which preserves earlier behavior.

With **One subprocess per FX chain** selected:

* each Carla chain has an independent worker process;
* the Carla external UI runs in the same worker as its LV2 instance;
* a failed or late worker produces silence for that chain's wet result while dry routing and other chains continue;
* clicking the FX button after a crash starts a new process generation, restores the last confirmed state and desired active state, and opens the Carla UI;
* session saves retain the last confirmed Carla state if a worker is unavailable.

The track menu's **Carla Process Logs...** action shows separate bounded stdout and stderr captures. Generation headings distinguish restarts. A dropped-byte count greater than zero means older output was evicted. The window can refresh, copy, and clear both streams.

The FX indicator is orange while a worker starts or restarts, red after a crash or startup failure, grey while bypassed, and uses the normal foreground color while active. Normal external-UI closure, session unload, and application shutdown are not treated as crashes.
