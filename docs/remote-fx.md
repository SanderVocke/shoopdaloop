# Remote Carla FX

A Native session host can process Carla Rack, Patchbay, and Patchbay 16x on a desktop FX service. The Native host continues to own routing, loop media, and the sample clock. Carla editors and plugin files live on the service machine, which can also run the ShoopDaLoop UI.

Use the Carla-enabled Native companion package on the desktop. The Native looper host also needs a Carla-enabled companion binary for the remote provider. It loads the saved projects on the service through the existing Carla state format.

## Start through Host Manager

1. On the desktop, open **Host Manager → FX services**. Set the listen address and start the service. Enable **LAN access** when the service needs to accept a looper from another machine.
2. On the looper machine, inspect the desktop's service address and choose **Use for new host…**. In the Native host startup form, choose the Carla chain counts and audio settings and start the host.
3. Attach the UI to that Native host. Add a dry/wet track or processed bus and select an available Carla type. Its FX control opens the editor on the service desktop.

The service reserves active and session-load candidate slots for every host. Its **FX services** entry shows reserved and active chains. Closing either user interface leaves the services and session running. Select **Stop service** from the machine hosting the service when you want to end it.

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

Control uses TCP on the configured port. Each prepared chain also uses an OS-assigned UDP port on the desktop and a UDP socket on the looper. Use a trusted network that allows both directions. The default service binding is loopback.

The looper groups one driver period of graph quanta per DSP batch and defaults to two additional batches of slack. At 48 kHz with a 128-frame period and a 32-frame graph quantum this schedules wet returns 384 frames after their source, or 8 ms of node buffering. `--carla-fx-slack-quanta` selects extra slack for a new host. Driver buffering is additional. The transport sends two copies per datagram by default.

## Session state and failure

Session files retain Carla chain kinds and their confirmed projects. Placement belongs to the Native host. The same file can be loaded by a compatible local Carla configuration or a remote service with the required plugins and resources. Session loads prepare a candidate before replacing the live session.

The FX state shows confirmed activity and service health. Missed wet returns follow the Native buffered processor's silence and MIDI recovery policy. An interrupted wet capture keeps the previous take. The looper reconnects to the configured service under a fresh stream identity and restores its last confirmed checkpoint. [Wire protocol](design/remote-fx-wire.md)
