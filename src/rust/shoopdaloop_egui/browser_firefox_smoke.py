#!/usr/bin/env python3
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
    options.set_preference("media.navigator.streams.fake", True)
    options.set_preference("media.navigator.permission.disabled", True)
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
        stress = os.environ.get("STRESS") == "1"
        driver.get(f"http://{HOST}:{PORT}/?self-test=1{'&stress=1' if stress else ''}")
        deadline = time.monotonic() + 20
        initial_state = {}
        while time.monotonic() < deadline:
            status = driver.find_element("id", "runtime_status")
            initial_state = {
                "text": status.text,
                "self_test": status.get_attribute("data-self-test"),
                "driver": status.get_attribute("data-driver-state"),
                "source": driver.page_source[:200],
            }
            if initial_state["self_test"] == "awaiting-audio":
                break
            time.sleep(0.1)
        else:
            raise RuntimeError(f"Firefox did not present the enable-audio action: {initial_state}")
        driver.find_element("id", "enable_audio").click()
        deadline = time.monotonic() + (360 if stress else 120)
        state = {}
        while time.monotonic() < deadline:
            status = driver.find_element("id", "runtime_status")
            state = {
                "self_test": status.get_attribute("data-self-test"),
                "driver": status.get_attribute("data-driver-state"),
                "self_test_error": status.get_attribute("data-self-test-error"),
                "callbacks": int(status.get_attribute("data-callback-count") or 0),
                "input_peak": float(status.get_attribute("data-input-peak") or 0),
                "output_peak": float(status.get_attribute("data-output-peak") or 0),
                "quantum": int(status.get_attribute("data-render-quantum") or 0),
                "overflows": int(status.get_attribute("data-command-overflows") or 0),
                "budget_overruns": int(
                    status.get_attribute("data-callback-budget-overruns") or 0
                ),
                "owned_media_tracks": int(
                    status.get_attribute("data-owned-media-tracks") or 0
                ),
                "web_midi": status.get_attribute("data-web-midi"),
                "web_midi_button": driver.find_element(By.ID, "enable_midi").text,
                "midi_host_ports": int(
                    status.get_attribute("data-midi-host-ports") or 0
                ),
            }
            if state["self_test"] == "passed":
                break
            if state["self_test"] == "failed" or state["driver"] in {"Denied", "Failed"}:
                raise RuntimeError(f"Firefox browser audio failed: {state}")
            time.sleep(0.1)
        else:
            raise RuntimeError(f"Firefox browser audio timed out: {state}")
        unsupported_midi_is_visible = (
            state["web_midi"] != "Unsupported"
            or "unsupported" in state["web_midi_button"].lower()
        )
        if not (
            state["driver"] == "Running"
            and state["callbacks"] > 0
            and state["input_peak"] > 0
            and state["output_peak"] > 0
            and state["quantum"] > 0
            and state["overflows"] == 0
            and state["budget_overruns"] == 0
            and state["owned_media_tracks"] > 0
            and state["web_midi"] in {"Unsupported", "AwaitingGesture"}
            and unsupported_midi_is_visible
            and state["midi_host_ports"] == 0
        ):
            raise RuntimeError(f"Firefox browser evidence is incomplete: {state}")
        print(f"Firefox Web Audio self-test passed: {state}")
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
