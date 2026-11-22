# Remote Carla FX

A Native session host can process Carla Rack, Patchbay, and Patchbay 16x on a desktop FX service. The Native host continues to own routing, loop media, and the sample clock. Carla editors and plugin files live on the service machine, which can also run the ShoopDaLoop UI.

The default Native build includes Carla support. Use companions from the same build on the desktop and looper. Use the `-carla` package on the machine that runs the FX service, or install the Carla runtime alongside its standard package. Install plugins on that machine. [Carla runtime layout](../third_party/carla/README.md)

## Start from Add Track

When connected to a remote Native session in the desktop UI, choose **Add track → Dry/wet → Remote Carla** and click **Run FX on this computer**. Shoop finds the desktop interface that routes to the session host, reuses an FX host listening there or starts one, and fills in its address. Click **Add** to open the chain from the session host. The FX host remains available in Host Manager after you close Shoop.

The session host must be able to reach that desktop interface over TCP and UDP. This works on a trusted LAN or VPN with connectivity in both directions. You can also select a discovered FX host or enter an address the session host can reach.

## Start from Host Manager

1. On the desktop, open **Host Manager → New host… → FX host** and select **Start host**. Enable **LAN access** if the looper runs on another machine. **One process per chain** enables supervised Carla subprocesses.
2. Select the FX host from the main list in Host Manager on the looper machine and choose **Start session host…**. The FX host is selected for the new session’s tracks and buses.
3. In Shoop, use **Add track → Dry/wet** and select a **Remote Carla** processor, or select one from a bus’s FX menu. The host opens the chain when you apply the change. Use the regular **Carla** entries for local processing where Carla is installed.

You can also choose a service from **Add track** in an existing session. A track can use the host’s service or choose a different one. Host Manager and the desktop picker discover services on the same machine and on the IPv4 LAN. For a routed or IPv6-only network, use **Advanced → Connect to address…** in Host Manager or enter the address in the track picker. The service address must be reachable from the Native host.

Carla workers and their buffers are created on demand. Session loads stage their own chains before committing the new graph. Deleting a track, removing a bus's FX, or replacing a session releases the corresponding chains. The FX control in Shoop opens and closes the editor on the service machine. In Host Manager, select the FX host to see its current chains and negotiated widths. On that machine, use the chain's editor control or **••• → Stop host…**. Closing either interface leaves the services and session running.

## Start from the command line

On the desktop, bind the FX service to its address on the trusted network.

```sh
shoop_host --fx-service --lan --listen 192.168.1.50:24510
```

For sessions and buses that use a host-wide FX service, set a default endpoint on the Native looper.

```sh
shoop_host --backend native --carla-fx-address 192.168.1.50:24510
```

The address in an individual track takes precedence over the host default. Start the FX service with `--isolated` for one supervised Carla worker process per chain. The default runs Carla in the service process with a dedicated media thread for each chain.

Control uses TCP on the configured port. Each chain uses an OS-assigned UDP port on the desktop and a UDP socket on the looper. Input and output widths are negotiated separately from the track or bus layout. Rack and stereo Patchbay support up to two lanes on each side and Patchbay 16x supports up to sixteen. Use a trusted network that allows both directions. The default service binding is loopback.

The looper groups graph quanta into DSP batches based on the driver period and schedules two additional batches of slack. At 48 kHz with a 128-frame period and a 32-frame graph quantum this schedules wet returns 384 frames after their source, or 8 ms of node buffering. Driver buffering is additional. The transport sends two copies per datagram by default.

## Session state and failure

Session files retain Carla chain kinds and their confirmed projects. A remote track also saves its selected service address and disconnect behavior. Earlier sessions that use a host-wide endpoint still use that default. The required plugin resources must be installed wherever the chain runs.

The **On disconnect** setting in Add Track controls recovery. For a host-wide endpoint, pass `--carla-fx-reconnect` with one of these values.

- `recreate` opens a new chain and restores the last confirmed checkpoint. This is the default.
- `resume` first tries to reattach to the running chain on the service. The service retains a disconnected chain for 30 seconds once its control connection ends, and reattachment gives it a fresh UDP identity. If the chain is gone, the host recreates it from the last confirmed checkpoint.
- `manual` waits for you to use the affected track or bus's FX control to retry. It tries the retained chain first, then recreates it from the last confirmed checkpoint if needed.

Missing wet audio is silent by default. Enable **Play delayed dry audio when FX is missing** or pass `--carla-fx-dry-on-loss` to hear the matching dry lanes at the same scheduled delay. Both modes recover MIDI notes. An interrupted wet capture keeps the previous take. [Wire protocol](design/remote-fx-wire.md)
