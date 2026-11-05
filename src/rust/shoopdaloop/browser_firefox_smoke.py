#!/usr/bin/env python3
"""Firefox smoke for genuine AudioWorklet startup and callback progress.

With --backend new_engine it runs the core workflow on the new engine backend
instead, through the application's in-browser self-test, with Firefox's fake
microphone as capture input.
"""

import argparse
import contextlib
import os
import subprocess
import time

from selenium import webdriver
from selenium.webdriver.common.by import By
from selenium.webdriver.firefox.options import Options

HOST = "127.0.0.1"
PORT = 8766


def status_of(driver) -> dict:
    status = driver.find_element(By.ID, "runtime_status")
    return {
        "driver": status.get_attribute("data-driver-state"),
        "new_engine": status.get_attribute("data-new-engine"),
        "microphone": status.get_attribute("data-new-engine-microphone"),
        "self_test": status.get_attribute("data-new-engine-self-test"),
        "input_test": status.get_attribute("data-input-self-test"),
        "frames": int(status.get_attribute("data-processed-frames") or 0),
        "sample_rate": int(status.get_attribute("data-sample-rate") or 0),
        "quantum": int(status.get_attribute("data-render-quantum") or 0),
        "xruns": int(status.get_attribute("data-xruns") or 0),
        "host_ports": int(status.get_attribute("data-host-ports") or 0),
        "status": status.text,
    }


def new_engine_workflow(driver) -> None:
    driver.get(f"http://{HOST}:{PORT}/?audio-backend=new_engine&new-engine-self-test=1")
    deadline = time.monotonic() + 30
    state = {}
    while time.monotonic() < deadline:
        state = status_of(driver)
        if state["driver"] == "AwaitingGesture" and state["new_engine"] == "AwaitingGesture":
            break
        time.sleep(0.1)
    else:
        raise RuntimeError(f"Firefox did not present the new engine's audio action: {state}")

    driver.find_element(By.ID, "enable_audio").click()
    deadline = time.monotonic() + 300
    last_step = None
    input_stalled = False
    while time.monotonic() < deadline:
        state = status_of(driver)
        if not input_stalled and state["input_test"] == "Stall":
            driver.execute_script("""
                const end = performance.now() + 2000;
                while (performance.now() < end) {}
                document.getElementById('runtime_status').setAttribute('data-input-stalled', 'true');
            """)
            input_stalled = True
        outputs = driver.find_elements(By.ID, "rfd-output")
        if outputs:
            if state["self_test"] == "CancelSaving":
                buttons = driver.find_elements(By.CSS_SELECTOR, "#rfd-card .rfd-button")
                next(button for button in buttons if button.text == "Cancel").click()
            else:
                outputs[0].click()
        if state["self_test"] != last_step:
            last_step = state["self_test"]
            print(f"new engine self-test: {last_step} ({state['status']})", flush=True)
        if state["self_test"] == "passed":
            break
        if (state["self_test"] or "").startswith("failed"):
            raise RuntimeError(f"Firefox new engine workflow failed: {state}")
        time.sleep(0.1)
    else:
        raise RuntimeError(f"Firefox new engine workflow timed out: {state}")

    frames = state["frames"]
    time.sleep(1)
    state = status_of(driver)
    if not (
        state["new_engine"] == "Running"
        and state["driver"] == "Running"
        and state["microphone"] == "Connected"
        and state["frames"] > frames
        and state["quantum"] == 128
        and state["host_ports"] >= 2
        and input_stalled
        and state["input_test"] == "Passed"
    ):
        raise RuntimeError(f"Firefox new engine evidence is incomplete: {state}")
    print(f"Firefox new engine workflow passed: {state}")


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--backend", choices=("new_engine",), default="new_engine")
    args = parser.parse_args()
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
    if args.backend == "new_engine":
        options.set_preference("media.navigator.streams.fake", True)
        options.set_preference("media.navigator.permission.disabled", True)
    if binary := os.environ.get("FIREFOX_BIN"):
        options.binary_location = binary
    driver = None
    try:
        driver = webdriver.Firefox(options=options)
        driver.set_window_size(900, 600)
        if args.backend == "new_engine":
            new_engine_workflow(driver)
            return
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
