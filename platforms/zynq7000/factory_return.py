"""Fence factory Linux entry behind a fresh full reset from custom PL."""
import time


def return_to_factory(console, read_word, enter_linux, *, capture=lambda _: None,
                      timeout=40, clock=time.monotonic, pause=time.sleep):
    if read_word(0xf800025c) & 15 != 5:
        raise RuntimeError("Factory return requires SD boot mode")
    if read_word(0xf8005000) & 3 != 3:
        raise RuntimeError("Keep the reset watchdog armed before factory return")
    if read_word(0xf8000258) & 0x0f000000:
        raise RuntimeError("Factory PL reload flags must be cleared before reset")
    console.receive()
    console.send(b"reset\r")
    deadline = clock() + timeout
    seen = bytearray()
    interrupted = aborted = False
    while clock() < deadline:
        seen.extend(console.receive())
        capture(bytes(seen))
        if b"BOOTP broadcast" in seen and not aborted and not interrupted:
            console.send(b"\x03")
            aborted = True
        if b"Hit any key to stop autoboot" in seen and not interrupted:
            console.send(b" ")
            interrupted = True
        countdown = seen.find(b"Hit any key to stop autoboot")
        if interrupted and seen.find(b"Zynq> ", countdown + len(b"Hit any key to stop autoboot")) >= 0:
            if read_word(0xf800025c) & 15 != 5:
                raise RuntimeError("Boot mode changed after reset; stopped in U-Boot")
            enter_linux(console)
            return
        pause(.02)
    raise TimeoutError("No fresh reset/countdown/prompt; no Linux boot or reset retry")
