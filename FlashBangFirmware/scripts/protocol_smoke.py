#!/usr/bin/env python3
import argparse
import glob
import os
import select
import termios
import time


def configure_serial(fd: int) -> None:
    attrs = termios.tcgetattr(fd)
    attrs[0] = 0
    attrs[1] = 0
    attrs[2] = termios.CS8 | termios.CREAD | termios.CLOCAL
    attrs[3] = 0
    attrs[4] = termios.B115200
    attrs[5] = termios.B115200
    attrs[6][termios.VMIN] = 0
    attrs[6][termios.VTIME] = 0
    termios.tcsetattr(fd, termios.TCSANOW, attrs)


def read_lines(fd: int, window_s: float) -> list[str]:
    end = time.time() + window_s
    buf = b""
    lines: list[str] = []
    while time.time() < end:
        ready, _, _ = select.select([fd], [], [], 0.05)
        if not ready:
            continue
        try:
            chunk = os.read(fd, 4096)
        except BlockingIOError:
            continue
        if not chunk:
            continue
        buf += chunk
        while b"\n" in buf:
            line, buf = buf.split(b"\n", 1)
            text = line.replace(b"\r", b"").decode("utf-8", errors="replace").strip()
            if text:
                lines.append(text)
    return lines


def run() -> int:
    parser = argparse.ArgumentParser(description="FlashBang protocol smoke test")
    parser.add_argument("--port", help="Serial port (default: latest /dev/ttyACM*)")
    parser.add_argument(
        "--destructive",
        action="store_true",
        help="Include mutating commands (program/erase). Use only on disposable data.",
    )
    args = parser.parse_args()

    ports = sorted(glob.glob("/dev/ttyACM*"))
    if not ports:
        print("NO_PORT: no /dev/ttyACM* found")
        return 1

    port = args.port if args.port else ports[-1]
    fd = os.open(port, os.O_RDWR | os.O_NOCTTY | os.O_NONBLOCK)
    try:
        configure_serial(fd)
        _ = read_lines(fd, 0.25)

        tests = [
            ("help", "?"),
            ("hello-cmd", "HELLO|host-test|0.1"),
            ("id", "ID"),
            ("read16", "READ|00000|16"),
            ("read-oob", "READ|80000|1"),
            ("malformed", "READ|GGGG|10"),
            # --- Driver-based communication tests ---
            ("inspect", "INSPECT"),
            ("sequence-set", "SEQUENCE|PROGRAM_BYTE|W5555,AA;W2AAA,55;W5555,A0;W$A,$D;T$A,50000"),
            ("param-builtin", "PARAMETER|CHIP_SIZE|80000"),
            ("param-custom", "PARAMETER|page_size|80"),
            ("inspect-after", "INSPECT"),
            ("driver-reset", "DRIVER_RESET"),
            ("inspect-reset", "INSPECT"),
            # Bad inputs
            ("seq-empty", "SEQUENCE||"),
            ("seq-toolong-name", "SEQUENCE|" + "x" * 25 + "|W5555,AA"),
            ("param-bad-val", "PARAMETER|CHIP_SIZE|ZZZZ"),
            ("unknown-custom", "nonexistent_seq"),
        ]

        if args.destructive:
            tests.extend(
                [
                    ("program-byte", "PROGRAM_BYTE|00000|AA"),
                    ("write-status", "WRITE_STATUS|00000|AA|20"),
                    ("sector-erase", "SECTOR_ERASE|00000"),
                    ("chip-erase", "CHIP_ERASE"),
                ]
            )

        print(f"PORT={port}")
        for name, cmd in tests:
            os.write(fd, (cmd + "\n").encode("ascii"))
            time.sleep(0.03)
            lines = read_lines(fd, 0.80)
            print(f"[{name}] CMD={cmd}")
            if not lines:
                print("  RESP=<none>")
                continue
            for line in lines:
                print(f"  RESP={line}")

    finally:
        os.close(fd)

    return 0


if __name__ == "__main__":
    raise SystemExit(run())
