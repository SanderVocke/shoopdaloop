#!/usr/bin/env python3
"""Exercise installed Remote Carla roles over an isolated virtual Ethernet link."""

import argparse
import asyncio
import json
import random
import os
from pathlib import Path
import platform
import socket
import subprocess
import sys
import tempfile
import time

LOOPER = "192.0.2.1"
DESKTOP = "192.0.2.2"
SERVICE_PORT = 24510


class Datagrams(asyncio.DatagramProtocol):
    def __init__(self, forward, pass_packet):
        self.forward = forward
        self.pass_packet = pass_packet

    def datagram_received(self, packet, address):
        if self.pass_packet(packet):
            self.forward(packet, address)


def impairment(mode, seed):
    first = set()
    offset = random.Random(seed).randrange(37)
    block_file = os.environ.get("SHOOP_REMOTE_FX_BLOCK_FILE")

    def pass_packet(packet):
        if len(packet) < 60 or packet[:3] != b"SFX":
            return True
        sequence = int.from_bytes(packet[28:36], "little")
        if mode == "capture":
            return not Path(block_file).exists()
        if mode == "burst":
            return (sequence + offset) % 67 not in (41, 42)
        if (sequence + offset) % 37 != 36:
            return True
        key = packet[:60]
        if key in first:
            first.remove(key)
            return True
        if len(first) >= 2048:
            first.clear()
        first.add(key)
        return False

    return pass_packet


async def read_frame(reader):
    length = int.from_bytes(await reader.readexactly(4), "little")
    if not 0 < length <= 4096:
        raise ValueError("invalid initial FX control frame")
    return await reader.readexactly(length)


async def write_frame(writer, data):
    writer.write(len(data).to_bytes(4, "little") + data)
    await writer.drain()


async def copy_stream(reader, writer):
    while data := await reader.read(64 * 1024):
        writer.write(data)
        await writer.drain()


async def relay_connection(down_reader, down_writer, mode):
    loop = asyncio.get_running_loop()
    up_writer = local = remote = None
    try:
        up_reader, up_writer = await asyncio.open_connection(DESKTOP, SERVICE_PORT)
        await write_frame(up_writer, await read_frame(down_reader))
        reply = json.loads(await read_frame(up_reader))
        if "ready" not in reply:
            await write_frame(down_writer, json.dumps(reply).encode())
            return

        peer = None

        def send_down(data, _):
            if peer is not None:
                local.sendto(data, peer)

        def send_up(data, address):
            nonlocal peer
            peer = address
            remote.sendto(data)

        remote, _ = await loop.create_datagram_endpoint(
            lambda: Datagrams(send_down, impairment(mode, 42)),
            remote_addr=(DESKTOP, reply["ready"]["udp_port"]),
        )
        local, _ = await loop.create_datagram_endpoint(
            lambda: Datagrams(send_up, impairment(mode, 91)),
            local_addr=("127.0.0.1", 0),
        )
        reply["ready"]["udp_port"] = local.get_extra_info("sockname")[1]
        await write_frame(down_writer, json.dumps(reply).encode())
        left = asyncio.create_task(copy_stream(down_reader, up_writer))
        right = asyncio.create_task(copy_stream(up_reader, down_writer))
        _, pending = await asyncio.wait([left, right], return_when=asyncio.FIRST_COMPLETED)
        for task in pending:
            task.cancel()
        await asyncio.gather(left, right, return_exceptions=True)
    except (ConnectionError, asyncio.IncompleteReadError):
        pass
    finally:
        for transport in (local, remote):
            if transport is not None:
                transport.close()
        for writer in (down_writer, up_writer):
            if writer is not None:
                writer.close()


async def relay(mode):
    server = await asyncio.start_server(
        lambda reader, writer: relay_connection(reader, writer, mode), "127.0.0.1", 0
    )
    print(server.sockets[0].getsockname()[1], flush=True)
    async with server:
        await server.serve_forever()


def service_record(reply):
    size = int.from_bytes(reply.read(4), "little")
    return json.loads(reply.read(size))


def service_request(intent):
    request = json.dumps({"version": 3, "intent": intent}).encode()
    with socket.create_connection((DESKTOP, SERVICE_PORT), timeout=1) as sock:
        sock.sendall(len(request).to_bytes(4, "little") + request)
        with sock.makefile("rb") as reply:
            response = service_record(reply)
            if intent == "describe":
                description = response["description"]
                description["chains"] = [service_record(reply) for _ in range(description["chain_count"])]
                return description
            return response


def query_service():
    return service_request("describe")


def shutdown_service():
    instance = query_service()["instance"]
    if service_request({"shutdown": {"instance": instance}}) != "shutdown":
        raise RuntimeError("desktop FX service did not confirm shutdown")


def run_archive(args, env, width, restart=None):
    env = {**env, "SHOOP_REMOTE_FX_WIDTH": str(width)}
    command = affinity(args.looper_cpus) + [args.archive_test, "--exact", "native_carla_session_archive_survives_driver_rate_and_period_change", "--nocapture"]
    sample = {"datagrams_per_second": 0.0, "peak_processing_us": 0, "peak_pending": 0}
    previous = None
    streams = []
    began = time.monotonic()
    with subprocess.Popen(command, env=env) as test:
        try:
            while test.poll() is None:
                if time.monotonic() - began > args.soak_seconds + 120:
                    raise TimeoutError(f"remote FX fixture exceeded its deadline for width {width}")
                marker = env.get("SHOOP_REMOTE_FX_RESTART_FILE")
                if marker and Path(marker).exists() and restart:
                    restart()
                    restart = None
                    previous = None
                streams = [
                    chain["status"]["counters"]
                    for chain in query_service()["chains"]
                    if chain["attached"]
                ]
                packets = sum(item["sent_datagrams"] + item["received_datagrams"] for item in streams)
                now = time.monotonic()
                if previous is not None and packets >= previous[0]:
                    sample["datagrams_per_second"] = max(
                        sample["datagrams_per_second"], (packets - previous[0]) / (now - previous[1])
                    )
                previous = (packets, now)
                for key in ("peak_processing_us", "peak_pending"):
                    sample[key] = max(sample[key], *(item[key] for item in streams)) if streams else sample[key]
                time.sleep(1)
            if test.returncode:
                print(f"Last desktop stream state for {width} channels {json.dumps(streams)}", file=sys.stderr)
                raise subprocess.CalledProcessError(test.returncode, command)
        finally:
            stop(test)
    print(f"Remote desktop metrics for {width} channels {json.dumps(sample, sort_keys=True)}", flush=True)


def run(command, **kwargs):
    return subprocess.run(command, check=True, **kwargs)


def affinity(cpus):
    return ["taskset", "--cpu-list", ",".join(map(str, cpus))] if cpus else []


def ready(address, process, logs):
    deadline = time.monotonic() + 20
    while time.monotonic() < deadline and process.poll() is None:
        try:
            with socket.create_connection(address, timeout=0.1):
                return
        except OSError:
            time.sleep(0.05)
    print(logs.read_text(errors="replace"), file=sys.stderr)
    raise RuntimeError(f"desktop FX service did not start at {address}")


def stop(process):
    if process.poll() is None:
        process.terminate()
        try:
            process.wait(timeout=5)
        except subprocess.TimeoutExpired:
            process.kill()
            process.wait()


def run_fixture(args, root):
    available_cpus = sorted(os.sched_getaffinity(0))
    args.looper_cpus = available_cpus[:2] if len(available_cpus) >= 4 else []
    desktop_cpus = available_cpus[2:4] if args.looper_cpus else []
    env = os.environ.copy()
    env.update(
        SHOOP_REMOTE_FX_HOST=str(args.host),
        SHOOP_REMOTE_FX_TEST_ADDRESS=f"{DESKTOP}:{SERVICE_PORT}",
        SHOOP_REQUIRE_CARLA_TESTS="1",
        SHOOP_REQUIRE_CARLA_UI_TESTS="1",
        SHOOP_TEST_CARLA_UI="1",
        SHOOP_REMOTE_FX_SOAK_SECONDS=str(args.soak_seconds),
        XDG_CONFIG_HOME=str(root / "config"),
    )
    if args.carla_runtime:
        env["SHOOP_CARLA_NATIVE_LIBRARY"] = str(
            args.carla_runtime / "lib/libcarla_native-plugin.so"
        )
        env["SHOOP_CARLA_RESOURCE_DIR"] = str(args.carla_runtime / "resources")
    else:
        env.pop("SHOOP_CARLA_NATIVE_LIBRARY", None)
        env.pop("SHOOP_CARLA_RESOURCE_DIR", None)
    if args.slack is not None:
        env["SHOOP_REMOTE_FX_SLACK"] = str(args.slack)

    run(["ip", "link", "set", "lo", "up"])
    if args.manager:
        run([args.manager, "--probe-fx-service"], env=env, timeout=60)

    peer = subprocess.Popen(["unshare", "--net", "sleep", "infinity"])
    broker = proxy = None
    log_path = root / "fx-service.log"
    try:
        deadline = time.monotonic() + 5
        own_namespace = os.readlink("/proc/self/ns/net")
        while os.readlink(f"/proc/{peer.pid}/ns/net") == own_namespace:
            if time.monotonic() >= deadline:
                raise RuntimeError("desktop network namespace did not start")
            time.sleep(0.01)
        enter = ["nsenter", "-t", str(peer.pid), "-n"]
        run(["ip", "link", "add", "fx-looper", "type", "veth", "peer", "name", "fx-desktop"])
        run(["ip", "link", "set", "fx-desktop", "netns", str(peer.pid)])
        run(["ip", "addr", "add", f"{LOOPER}/30", "dev", "fx-looper"])
        run(["ip", "link", "set", "fx-looper", "up"])
        run(["ip", "route", "add", "239.255.74.80/32", "dev", "fx-looper"])
        run(enter + ["ip", "link", "set", "lo", "up"])
        run(enter + ["ip", "addr", "add", f"{DESKTOP}/30", "dev", "fx-desktop"])
        run(enter + ["ip", "link", "set", "fx-desktop", "up"])

        def start_service():
            service_args = [str(args.host), "--fx-service", "--lan", "--listen", f"{DESKTOP}:{SERVICE_PORT}"]
            if args.isolated:
                service_args.append("--isolated")
            with log_path.open("a") as log:
                service = subprocess.Popen(
                    affinity(desktop_cpus) + enter + service_args,
                    env=env, stdout=log, stderr=subprocess.STDOUT,
                )
            try:
                ready((DESKTOP, SERVICE_PORT), service, log_path)
            except BaseException:
                stop(service)
                raise
            return service

        def restart_service():
            nonlocal broker
            stop(broker)
            time.sleep(0.75)
            broker = start_service()

        broker = start_service()
        if args.manager:
            run([args.manager, "--probe-fx-service"], env={
                **env, "SHOOP_FX_PROBE_DISCOVER_ADDRESS": env["SHOOP_REMOTE_FX_TEST_ADDRESS"],
            }, timeout=20)
        print(
            f"Remote Carla on {platform.platform()}, {len(os.sched_getaffinity(0))} CPUs, "
            f"{args.host}, virtual Ethernet, 48 kHz, 128-frame period, 32-frame quantum. "
            f"Looper CPUs {args.looper_cpus or 'shared'}, service CPUs {desktop_cpus or 'shared'}.",
            flush=True,
        )
        if args.restart:
            restart_env = {
                **env,
                "SHOOP_REMOTE_FX_RESTART_FILE": str(root / "restart"),
                "SHOOP_REMOTE_FX_SOAK_SECONDS": "0",
            }
            run_archive(args, restart_env, 2, restart_service)
        if args.recovery_test:
            run(
                affinity(args.looper_cpus) + [args.recovery_test, "--exact", "carla_editor_controls_follow_the_confirmed_track_and_bus", "--nocapture"],
                env=env, timeout=120,
            )
        if args.archive_test:
            run(
                affinity(args.looper_cpus) + [args.archive_test, "--exact", "remote_carla_track_selects_a_service_after_a_default_host_starts", "--nocapture"],
                env=env, timeout=30,
            )
        for width in args.width:
            run_archive(args, env, width)
        if args.sweep:
            for period, slack in ((64, 4), (128, 8), (128, 14), (256, 16)):
                sweep = {
                    **env,
                    "SHOOP_REMOTE_FX_PERIOD": str(period),
                    "SHOOP_REMOTE_FX_SLACK": str(slack),
                    "SHOOP_REMOTE_FX_SWEEP": "1",
                    "SHOOP_REMOTE_FX_SOAK_SECONDS": "5",
                }
                run_archive(args, sweep, 2)
        if args.impairment:
            for mode in dict.fromkeys((args.impairment, "burst", "capture")):
                block = str(root / "block-media")
                proxy = subprocess.Popen(
                    [sys.executable, str(Path(__file__).resolve()), "--relay", mode],
                    env={**env, "SHOOP_REMOTE_FX_BLOCK_FILE": block},
                    stdout=subprocess.PIPE, text=True,
                )
                port = int(proxy.stdout.readline())
                impaired = {
                    **env,
                    "SHOOP_REMOTE_FX_TEST_ADDRESS": f"127.0.0.1:{port}",
                    "SHOOP_REMOTE_FX_IMPAIRED": mode,
                    "SHOOP_REMOTE_FX_SOAK_SECONDS": "4" if mode != "capture" else "0",
                }
                if mode == "capture":
                    impaired["SHOOP_REMOTE_FX_BLOCK_FILE"] = block
                run_archive(args, impaired, 2)
                if mode == "one-copy":
                    run_archive(args, {**impaired, "SHOOP_REMOTE_FX_COPIES": "1"}, 2)
                stop(proxy)
                proxy = None
        run(enter + [sys.executable, str(Path(__file__).resolve()), "--shutdown-service"], timeout=5)
        if broker.wait(timeout=15) != 0:
            raise RuntimeError("desktop FX service failed during shutdown")
    except BaseException:
        if log_path.exists():
            print(log_path.read_text(errors="replace")[-20_000:], file=sys.stderr)
        raise
    finally:
        if proxy is not None:
            stop(proxy)
        if broker is not None:
            stop(broker)
        stop(peer)


def main():
    if len(sys.argv) == 3 and sys.argv[1] == "--relay":
        asyncio.run(relay(sys.argv[2]))
        return
    if sys.argv[1:] == ["--shutdown-service"]:
        shutdown_service()
        return
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--host", required=True, type=Path)
    parser.add_argument("--manager", type=Path)
    parser.add_argument("--carla-runtime", type=Path)
    parser.add_argument("--archive-test", required=True, type=Path)
    parser.add_argument("--recovery-test", type=Path)
    parser.add_argument("--soak-seconds", type=int, default=1800)
    parser.add_argument("--slack", type=int)
    parser.add_argument("--impairment", choices=["one-copy", "burst"])
    parser.add_argument("--restart", action="store_true")
    parser.add_argument("--isolated", action="store_true")
    parser.add_argument("--sweep", action="store_true")
    parser.add_argument("--width", nargs="+", type=int, choices=[2, 16], default=[2, 16])
    parser.add_argument("--network-ready", action="store_true", help=argparse.SUPPRESS)
    args = parser.parse_args()
    script = str(Path(__file__).resolve())
    if "DISPLAY" not in os.environ:
        os.execvp("xvfb-run", ["xvfb-run", "-a", sys.executable, script, *sys.argv[1:]])
    if not args.network_ready:
        namespace = ["unshare", "--net"]
        if os.geteuid() != 0:
            namespace.extend(["--user", "--map-root-user"])
        os.execvp("unshare", [*namespace, sys.executable, script, *sys.argv[1:], "--network-ready"])
    for name in ("host", "manager", "carla_runtime", "archive_test", "recovery_test"):
        value = getattr(args, name)
        if value is not None:
            setattr(args, name, value.resolve(strict=True))
    with tempfile.TemporaryDirectory(prefix="shoop-remote-fx-") as directory:
        run_fixture(args, Path(directory))


if __name__ == "__main__":
    main()
