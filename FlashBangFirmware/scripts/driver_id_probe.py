#!/usr/bin/env python3
import argparse
import os
import select
import termios
import time
from pathlib import Path

import yaml


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
            raw, buf = buf.split(b"\n", 1)
            line = raw.replace(b"\r", b"").decode("utf-8", errors="replace").strip()
            if line:
                lines.append(line)
    return lines


def send_line(fd: int, line: str) -> None:
    data = (line + "\n").encode("utf-8")
    sent = 0
    while sent < len(data):
        try:
            n = os.write(fd, data[sent:])
        except BlockingIOError:
            n = 0
        if n <= 0:
            time.sleep(0.001)
            continue
        sent += n


def to_upper_hex(value: int) -> str:
    return f"{value:X}"


def is_supported_sequence(script: str) -> bool:
    return script.strip().upper() != "UNSUPPORTED"


def ms_to_us_hex(ms: int) -> str:
    us = min(int(ms) * 1000, 0xFFFFFFFF)
    return f"{us:X}"


def build_upload_lines(driver: dict) -> list[str]:
    models = driver.get("models", [])
    if not models:
        raise ValueError("driver yaml has no models")

    size_bytes = int(models[0]["size_bytes"])
    sector_size = int(driver["sector_size_bytes"])
    address_bits = int(driver["address_bits"])

    seq = driver["sequences"]
    required = [
        "id_entry",
        "id_read",
        "id_exit",
        "program_byte",
        "program_range",
        "sector_erase",
        "chip_erase",
    ]
    for key in required:
        if key not in seq:
            raise ValueError(f"missing sequence: {key}")

    lines = [
        f"PARAMETER|CHIP_SIZE|{to_upper_hex(size_bytes)}",
        f"PARAMETER|SECTOR_SIZE|{to_upper_hex(sector_size)}",
        f"PARAMETER|ADDR_BITS|{to_upper_hex(address_bits)}",
        f"SEQUENCE|ID_ENTRY|{seq['id_entry']}",
        f"SEQUENCE|ID_READ|{seq['id_read']}",
        f"SEQUENCE|ID_EXIT|{seq['id_exit']}",
        f"SEQUENCE|PROGRAM_BYTE|{seq['program_byte']}",
    ]

    timing = driver.get("timing") or {}
    if "program_timeout_ms" in timing:
        lines.append(
            f"PARAMETER|program_timeout_us|{ms_to_us_hex(timing['program_timeout_ms'])}"
        )
    if "sector_erase_timeout_ms" in timing:
        lines.append(
            f"PARAMETER|sector_erase_timeout_us|{ms_to_us_hex(timing['sector_erase_timeout_ms'])}"
        )
    if "chip_erase_timeout_ms" in timing:
        lines.append(
            f"PARAMETER|chip_erase_timeout_us|{ms_to_us_hex(timing['chip_erase_timeout_ms'])}"
        )

    if is_supported_sequence(seq["program_range"]):
        lines.append(f"SEQUENCE|PROGRAM_RANGE|{seq['program_range']}")
    if is_supported_sequence(seq["sector_erase"]):
        lines.append(f"SEQUENCE|sector_erase|{seq['sector_erase']}")
    if is_supported_sequence(seq["chip_erase"]):
        lines.append(f"SEQUENCE|CHIP_ERASE|{seq['chip_erase']}")

    return lines


def resolve_driver_path(arg: str) -> Path:
    p = Path(arg)
    if p.exists():
        return p

    script_dir = Path(__file__).resolve().parent
    repo_root = script_dir.parent.parent
    candidate = repo_root / "drivers" / "chips" / f"{arg}.yaml"
    if candidate.exists():
        return candidate

    raise FileNotFoundError(f"driver not found: {arg}")


def run() -> int:
    parser = argparse.ArgumentParser(
        description="Upload driver PARAMETER/SEQUENCE lines from YAML, then run ID"
    )
    parser.add_argument(
        "--driver",
        default="sst39-core",
        help="Driver yaml path or driver id without .yaml (default: sst39-core)",
    )
    parser.add_argument("--port", default="/dev/ttyACM0", help="Serial port")
    parser.add_argument(
        "--rx-window",
        type=float,
        default=2.5,
        help="Seconds to collect responses after ID (default: 2.5)",
    )
    args = parser.parse_args()

    driver_path = resolve_driver_path(args.driver)
    driver = yaml.safe_load(driver_path.read_text(encoding="utf-8"))
    lines = build_upload_lines(driver)

    print(f"DRIVER_FILE={driver_path}")
    print(f"PORT={args.port}")
    print(f"UPLOAD_LINES={len(lines)}")

    fd = os.open(args.port, os.O_RDWR | os.O_NOCTTY | os.O_NONBLOCK)
    try:
        configure_serial(fd)
        _ = read_lines(fd, 0.3)

        for idx, line in enumerate(lines, 1):
            print(f"TX[{idx}]={line}")
            send_line(fd, line)
            for resp in read_lines(fd, 0.12):
                print(f"  RX={resp}")

        print("TX[ID]=ID")
        send_line(fd, "ID")

        responses = read_lines(fd, args.rx_window)
        if not responses:
            print("RX=<none>")
            return 2

        id_lines = []
        for resp in responses:
            print(f"RX={resp}")
            if resp.startswith("OK|ID|"):
                id_lines.append(resp)

        if not id_lines:
            print("RESULT=NO_ID_LINE")
            return 3

        print(f"RESULT=ID_OK|{id_lines[-1]}")
        return 0
    finally:
        os.close(fd)


if __name__ == "__main__":
    raise SystemExit(run())
