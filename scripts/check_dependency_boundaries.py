#!/usr/bin/env python3
"""Audit production/build dependencies and isolated optional DSP features."""

import argparse
from pathlib import Path
import re
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[1]
TARGETS = (
    "x86_64-unknown-linux-gnu", "x86_64-pc-windows-msvc",
    "aarch64-apple-darwin", "wasm32-unknown-unknown",
)
PORTABLE = set(
    "alsa alsa-sys cpal eframe egui gtk gtk-sys jack jack-sys js-sys midir rfd rodio wasm-bindgen "
    "web-sys winit".split()
)

ENGINE = set(
    "shoop_app shoop_app_api shoop_audio_protocol shoop_audio_worklet shoop_backend "
    "shoop_deployment_protocol shoop_egui shoop_engine shoop_host shoop_session "
    "shoop_worklet_client shoopdaloop".split()
)


def rule(forbidden=(), prefixes=(), allowed=()):
    return set(forbidden), prefixes, set(allowed)


RULES = {
    "profiling_protocol": rule(PORTABLE | {"rt_runtime", "rt_profiling", "freertos_profiling"}, prefixes=('shoop_',)),
    "freertos_profiling": rule(PORTABLE | {"rt_runtime", "rt_profiling"}, prefixes=('shoop_',)),
    "shoop_midi_protocol": rule(PORTABLE | {'rt_runtime', 'rt_profiling'}, prefixes=('shoop_',)),
    "rt_runtime rt_buffered": rule(PORTABLE, prefixes=('shoop_',), allowed=('shoop_midi_protocol',)),
    "shoop_loops": rule(PORTABLE, prefixes=('shoop_',), allowed=('shoop_loop_plan', 'shoop_loops', 'shoop_midi_protocol')),
    "shoop_loop_plan": rule(
        PORTABLE | {"rt_runtime"},
        prefixes=('shoop_',),
        allowed=('shoop_loop_plan',),
    ),
    "shoop_deployment_protocol shoop_app_protocol shoop_app_client": rule(
        (
            "cpal iceoryx2 js-sys midir rt_runtime shoop_app shoop_backend shoop_egui shoop_engine "
            "shoop_host shoop_scripting shoop_session shoopdaloop web-sys"
        ).split(),
        prefixes=('shoop_rt_', 'shoop_transport_'),
    ),
    "shoop_port_graph": rule(
        "cpal iceoryx2 jack jack-sys js-sys midir rt_runtime web-sys".split(),
        prefixes=('shoop_',),
        allowed=('shoop_port_graph',),
    ),
    "shoop_app_connection": rule(
        (
            "cpal iceoryx2 js-sys midir rt_runtime shoop_app_client shoop_egui shoop_engine "
            "shoop_host shoop_scripting shoop_session shoopdaloop web-sys"
        ).split(),
        prefixes=('shoop_rt_', 'shoop_transport_'),
    ),
    "shoopdaloop": rule(
        (
            "shoop_app_reference shoop_audio_protocol shoop_audio_worklet shoop_engine "
            "shoop_rt_host_process shoop_worklet_client"
        ).split(),
    ),
    "shoop_persistence_codec": rule(
        (
            "cpal iceoryx2 js-sys midir shoop_app shoop_app_client shoop_app_reference shoop_backend "
            "shoop_egui shoop_engine shoop_host shoop_scripting shoopdaloop web-sys"
        ).split(),
        prefixes=('shoop_rt_', 'shoop_transport_'),
    ),
    "shoop_host": rule(
        (
            "shoop_app_reference shoop_audio_protocol shoop_audio_worklet shoop_egui shoop_engine "
            "shoop_scripting shoop_session shoop_worklet_client shoopdaloop"
        ).split(),
    ),
    "shoop_host_freertos": rule(
        (
            "cpal iceoryx2 js-sys midir shoop_app_client shoop_app_reference shoop_egui shoop_engine "
            "shoop_host shoop_persistence_codec shoop_rt_host_native shoop_scripting shoop_session "
            "shoop_transport_shared_memory shoop_transport_socket shoopdaloop wasm-bindgen web-sys"
        ).split(),
    ),
    "shoop_app": rule(
        (
            "cpal iceoryx2 js-sys midir rt_runtime shoop_app_reference shoop_egui shoop_engine "
            "shoop_host shoop_scripting shoop_session shoopdaloop web-sys"
        ).split(),
        prefixes=('shoop_rt_', 'shoop_transport_'),
    ),
    "shoop_backend shoop_rt_backend": rule(
        (
            "cpal midir rt_runtime shoop_app shoop_app_client shoop_app_protocol shoop_egui "
            "shoop_engine shoop_host shoop_rt_engine shoop_scripting shoop_session shoopdaloop"
        ).split(),
        prefixes=('shoop_rt_host_', 'shoop_rt_transport_', 'shoop_transport_'),
    ),
    "shoop_transport": rule(
        ENGINE | set("cpal iceoryx2 js-sys midir rt_runtime shoop_loops wasm-bindgen web-sys".split()),
        prefixes=('shoop_rt_', 'shoop_transport_'),
    ),
    "shoop_transport_socket shoop_transport_shared_memory": rule(
        ENGINE | set("rt_runtime shoop_loops".split()),
        prefixes=('shoop_rt_',),
    ),
    "shoop_rt_engine": rule(
        ENGINE | set("fundsp oxisynth shoop_dsp shoop_host_processors soundfont".split()),
    ),
    "shoop_rt_protocol": rule(
        ENGINE | set((
            "cpal fundsp iceoryx2 js-sys midir oxisynth rt_runtime shoop_dsp shoop_host_processors "
            "shoop_loops shoop_rt_engine shoop_rt_transport soundfont wasm-bindgen web-sys"
        ).split()),
        prefixes=('shoop_rt_host_', 'shoop_rt_transport_', 'shoop_transport_'),
    ),
    "shoop_processor_protocol": rule(
        ENGINE | set((
            "cpal fundsp js-sys oxisynth rt_runtime shoop_dsp shoop_host_processors shoop_loops "
            "shoop_rt_engine soundfont std web-sys"
        ).split()),
    ),
    "shoop_rt_transport": rule(
        ENGINE | set("cpal iceoryx2 js-sys midir rt_runtime shoop_loops shoop_rt_engine wasm-bindgen web-sys".split()),
        prefixes=('shoop_rt_host_', 'shoop_rt_transport_', 'shoop_transport_'),
    ),
    "shoop_rt_host_native shoop_jack shoop_rt_host_worklet shoop_rt_host_process": rule(ENGINE),
    "shoop_rt_transport_message_port shoop_rt_transport_socket shoop_rt_transport_shared_memory": rule(
        ENGINE | set("rt_runtime shoop_loops shoop_rt_engine".split()),
    ),
    "shoop_app_client_runtime": rule(
        (
            "cpal iceoryx2 rt_runtime shoop_app shoop_backend shoop_egui shoop_engine shoop_host "
            "shoop_session shoopdaloop"
        ).split(),
        prefixes=('shoop_rt_', 'shoop_transport_'),
    ),
    "shoop_dsp": rule(
        (
            "cpal iceoryx2 jack jack-sys midir shoop_app shoop_app_api shoop_app_reference "
            "shoop_backend shoop_egui shoop_engine shoop_scripting shoop_session shoopdaloop"
        ).split(),
    ),
}
POLICIES = {package: policy for packages, policy in RULES.items() for package in packages.split()}

SCOPES = {
    "runtime": ("rt_runtime", "rt_buffered"),
    "loops": ("shoop_loop_plan", "shoop_loops"),
    "engine": (
        "profiling_protocol",
        "freertos_profiling",
        'shoop_deployment_protocol',
        'shoop_port_graph',
        'shoop_app_connection',
        'shoopdaloop',
        'shoop_persistence_codec',
        'shoop_host',
        'shoop_host_freertos',
        'shoop_app',
        'shoop_backend',
        'shoop_app_protocol',
        'shoop_app_client',
        'shoop_transport',
        'shoop_transport_socket',
        'shoop_transport_shared_memory',
        'shoop_rt_engine',
        'shoop_rt_protocol',
        'shoop_processor_protocol',
        'shoop_rt_transport',
        'shoop_rt_host_native',
        'shoop_jack',
        'shoop_rt_host_worklet',
        'shoop_rt_transport_message_port',
        'shoop_rt_transport_socket',
        'shoop_rt_transport_shared_memory',
        'shoop_rt_host_process',
        'shoop_rt_backend',
    ),
    "dsp": ("shoop_dsp",),
}
NO_DEFAULTS = {"shoop_app", "shoop_backend", "shoop_app_client", "shoop_app_connection"}
DSP_FEATURES = {"builtin-fx": {"fundsp"}, "oxisynth": {"oxisynth", "soundfont", "oxisynth-chorus", "oxisynth-reverb"}}


def violations(tree, package, features=()):
    names = set(re.findall(r"^([A-Za-z0-9_-]+) v\d", tree, re.MULTILINE))
    forbidden, prefixes, allowed = POLICIES[package]
    invalid = names & forbidden
    invalid.update(name for name in names if name.startswith(prefixes) and name not in allowed)
    if package == "shoop_dsp":
        for feature, dependencies in DSP_FEATURES.items():
            if feature not in features:
                invalid.update(names & dependencies)
    return sorted(invalid)


def checks(package):
    if package == "shoop_dsp":
        return [(package, features, package) for features in
                ((), ("builtin-fx",), ("oxisynth",), ("builtin-fx", "oxisynth"))]
    result = [(package, () if package in NO_DEFAULTS else None, package)]
    if package == "shoop_app_client":
        result.append((package, ("runtime",), "shoop_app_client_runtime"))
    return result


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--target", action="append", help="Override the four-target audit")
    parser.add_argument("--scope", action="append", choices=SCOPES, help="Audit only selected layers")
    parser.add_argument("--package", action="append", choices=sorted(set().union(*SCOPES.values())))
    args = parser.parse_args(argv)
    packages = list(dict.fromkeys(p for scope in args.scope or SCOPES for p in SCOPES[scope]))
    if args.package:
        packages = [p for p in packages if p in args.package]
        if set(args.package) - set(packages):
            parser.error("requested package is outside the selected scopes")
    failed = False
    for package in packages:
        for owner, features, policy in checks(package):
            for target in args.target or TARGETS:
                flags = (["--all-features"] if features is None else
                         [] if policy == "shoop_app_client_runtime" else ["--no-default-features"])
                if features:
                    flags += ["--features", ",".join(features)]
                command = ["cargo", "tree", "--locked", "-p", owner, *flags, "--target", target,
                           "-e", "normal,build", "--prefix", "none", "--format", "{p}"]
                result = subprocess.run(command, cwd=ROOT, text=True, capture_output=True, check=False)
                if result.returncode:
                    sys.stderr.write(result.stdout + result.stderr)
                    return result.returncode
                if not re.search(rf"^{re.escape(owner)} v\d", result.stdout, re.MULTILINE):
                    print(f"{target}: missing {owner} root", file=sys.stderr)
                    return 1
                invalid = violations(result.stdout, policy, features or ())
                feature_label = "all" if features is None else ",".join(features) or "minimal"
                label = f"{target}: {policy} ({feature_label})"
                if invalid:
                    print(f"{label}: forbidden dependencies: {', '.join(invalid)}", file=sys.stderr)
                    failed = True
                else:
                    print(f"{label}: production/build boundary: ok")
    return int(failed)


if __name__ == "__main__":
    raise SystemExit(main())
