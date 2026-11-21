# Remote Carla FX

A Native session host can process Carla Rack, Patchbay, and Patchbay 16x on a desktop FX service. The Native host continues to own routing, loop media, and the sample clock. Carla editors and plugin files live on the service machine, which can also run the ShoopDaLoop UI.

Use Carla-enabled Native companions from the same build on the desktop and looper. The looper loads saved projects on the service through the existing Carla state format.

## Start through Host Manager

1. On the desktop, open **Host Manager → FX services**. Set the listen address and start the service. Enable **LAN access** when the service needs to accept a looper from another machine. **One process per chain** enables supervised Carla subprocesses.
2. On the looper machine, inspect the desktop's service address and choose **Use for new host…**. Choose **Remote FX service** or **Local and remote Carla**, then set the per-type instance limits and audio settings. Mixed mode applies the configured limits separately to local and remote processors.
3. Attach the UI to that Native host. Add a dry/wet track or set the FX on a bus, and select an available Carla type. In mixed mode, choose a **Remote Carla** entry to run it on the service. The track or bus FX control opens its editor on the machine that runs that chain.

The Native host prepares bounded audio buffers at startup. It opens a connection and creates a remote chain when a track or bus needs it. Session loads stage their own chains before committing the new graph. Deleting a track, removing a bus's FX, or replacing a session releases the corresponding chains. Host Manager lists current chains and their negotiated audio widths. From Host Manager on the service machine, use **Open editor** or **Close editor** for an individual chain, or **Stop service** to stop the service and its chains. Closing either user interface leaves the services and session running.

## Start from the command line

On the desktop, bind the FX service to its address on the trusted network.

```sh
shoop_host --fx-service --lan --listen 192.168.1.50:24510
```

On the Native looper, select remote hosting and allocate one Rack chain.

```sh
shoop_host --backend native --carla-hosting remote \
  --carla-fx-address 192.168.1.50:24510 --carla-rack-instances 1
```

Use `--carla-hosting mixed` to expose local and remote Carla selections in one session. Start the FX service with `--isolated` for one supervised Carla worker process per chain. The default runs Carla in the service process with a dedicated media thread for each chain.

Control uses TCP on the configured port. Each chain uses an OS-assigned UDP port on the desktop and a UDP socket on the looper. Input and output widths are negotiated separately from the track or bus layout. Rack and stereo Patchbay support up to two lanes on each side and Patchbay 16x supports up to sixteen. Use a trusted network that allows both directions. The default service binding is loopback.

The looper groups one driver period of graph quanta per DSP batch and defaults to two additional batches of slack. At 48 kHz with a 128-frame period and a 32-frame graph quantum this schedules wet returns 384 frames after their source, or 8 ms of node buffering. `--carla-fx-slack-quanta` selects extra slack for a new host. Driver buffering is additional. The transport sends two copies per datagram by default.

## Session state and failure

Session files retain Carla chain kinds and their confirmed projects. Explicit remote processor selections in a mixed session are saved with the track or bus. Sessions that use the original Carla processor identifiers follow the Native host's selected mode. Load a mixed session on a host with both providers enabled and the required plugin resources present.

The **On service disconnect** setting controls recovery. From the command line, pass `--carla-fx-reconnect` with one of these values.

- `recreate` opens a new chain and restores the last confirmed checkpoint. This is the default.
- `resume` first tries to reattach to the running chain on the service. The service retains a disconnected chain for 30 seconds once its control connection ends, and reattachment gives it a fresh UDP identity. If the chain is gone, the host recreates it from the last confirmed checkpoint.
- `manual` waits for you to use the affected track or bus's FX control to retry. It tries the retained chain first, then recreates it from the last confirmed checkpoint if needed.

Missing wet audio is silent by default. Enable **Play delayed dry audio when FX is missing** or pass `--carla-fx-dry-on-loss` to hear the matching dry lanes at the same scheduled delay. Both modes recover MIDI notes. An interrupted wet capture keeps the previous take. [Wire protocol](design/remote-fx-wire.md)
