#!/usr/bin/env python3
"""Minimal Firefox smoke for genuine AudioWorklet startup and callback progress."""

import contextlib
import os
import subprocess
import time

from selenium import webdriver
from selenium.webdriver.common.by import By
from selenium.webdriver.firefox.options import Options

HOST = "127.0.0.1"
PORT = 8766


def main() -> None:
    server = subprocess.Popen(
        ["python3", "-m", "http.server", str(PORT), "--bind", HOST],
        cwd="dist",
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
    )
    options = Options()
    if not os.environ.get("DISPLAY"):
        options.add_argument("-headless")
    options.set_preference("media.autoplay.default", 0)
    options.set_preference("media.autoplay.blocking_policy", 0)
    options.set_preference("webgl.disabled", False)
    options.set_preference("webgl.force-enabled", True)
    options.set_preference("webgl.software", True)
    options.set_preference("gfx.webrender.software", True)
    if binary := os.environ.get("FIREFOX_BIN"):
        options.binary_location = binary
    driver = None
    try:
        driver = webdriver.Firefox(options=options)
        driver.set_window_size(900, 600)
        driver.get(f"http://{HOST}:{PORT}/")

        deadline = time.monotonic() + 20
        initial_state = {}
        while time.monotonic() < deadline:
            status = driver.find_element(By.ID, "runtime_status")
            initial_state = {
                "driver": status.get_attribute("data-driver-state"),
                "revision": int(
                    status.get_attribute("data-engine-revision") or 0
                ),
                "status": status.text,
                "body": driver.find_element(By.TAG_NAME, "body").text[:300],
            }
            output_actions = driver.find_elements(By.ID, "enable_output_audio")
            diagnostics_ready = driver.execute_script(
                "return typeof window.shoopAudioDiagnostics !== 'undefined' && "
                "typeof document.getElementById('enable_output_audio').onclick === 'function'"
            )
            if (
                diagnostics_ready
                and output_actions
                and output_actions[0].is_displayed()
                and output_actions[0].is_enabled()
            ):
                break
            time.sleep(0.1)
        else:
            raise RuntimeError(
                f"Firefox did not present the output-audio action: {initial_state}"
            )

        driver.find_element(By.ID, "enable_output_audio").click()
        deadline = time.monotonic() + 120
        state = {}
        while time.monotonic() < deadline:
            driver.execute_script(
                "window.shoopAudioDiagnostics?.poll(new Event('poll'))"
            )
            status = driver.find_element(By.ID, "runtime_status")
            state = {
                "driver": status.get_attribute("data-driver-state"),
                "startup_stage": status.get_attribute("data-audio-startup-stage"),
                "callbacks": int(status.get_attribute("data-callback-count") or 0),
                "frames": int(status.get_attribute("data-processed-frames") or 0),
                "quantum": int(status.get_attribute("data-render-quantum") or 0),
                "overflows": int(
                    status.get_attribute("data-command-overflows") or 0
                ),
                "owned_media_tracks": int(
                    status.get_attribute("data-owned-media-tracks") or 0
                ),
                "output_disabled": driver.find_element(
                    By.ID, "enable_output_audio"
                ).get_attribute("disabled"),
                "diagnostics": driver.execute_script(
                    "return window.shoopAudioDiagnostics || null"
                ),
            }
            if state["driver"] == "Running" and state["callbacks"] > 0:
                break
            if state["driver"] in {"Denied", "Failed"}:
                raise RuntimeError(f"Firefox browser audio failed: {state}")
            time.sleep(0.1)
        else:
            raise RuntimeError(f"Firefox browser audio timed out: {state}")

        if not (
            state["frames"] >= state["callbacks"] * 128
            and state["quantum"] == 128
            and state["overflows"] == 0
            and state["owned_media_tracks"] == 0
        ):
            raise RuntimeError(f"Firefox AudioWorklet evidence is incomplete: {state}")
        print(f"Firefox AudioWorklet smoke passed: {state}")
    finally:
        if driver is not None:
            with contextlib.suppress(Exception):
                driver.quit()
        server.terminate()
        with contextlib.suppress(subprocess.TimeoutExpired):
            server.wait(timeout=2)
        if server.poll() is None:
            server.kill()


if __name__ == "__main__":
    main()
