#!/usr/bin/env python3
import argparse
import glob
import os
import select
import sys
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


def find_port(explicit_port: str | None) -> str:
    if explicit_port:
        return explicit_port
    ports = sorted(glob.glob("/dev/ttyACM*"))
    if not ports:
        raise RuntimeError("no /dev/ttyACM* device found")
    return ports[-1]


def read_lines(fd: int, timeout_s: float) -> list[str]:
    end = time.time() + timeout_s
    lines: list[str] = []
    buf = b""
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


def parse_data_frame(line: str) -> tuple[int, int, bytes] | None:
    if not line.startswith("DATA|"):
        return None
    parts = line.split("|", 3)
    if len(parts) != 4:
        return None
    addr = int(parts[1], 16)
    size = int(parts[2], 10)
    data = bytes.fromhex(parts[3])
    if len(data) != size:
        raise RuntimeError(f"length mismatch in frame at 0x{addr:05X}")
    return addr, size, data


def run() -> int:
    parser = argparse.ArgumentParser(description="Dump ROM range via FlashBang protocol")
    parser.add_argument("--port", help="Serial port (default: latest /dev/ttyACM*)")
    parser.add_argument("--start", default="00000", help="Start address in hex (default: 00000)")
    parser.add_argument("--length", type=int, default=4096, help="Length in bytes (default: 4096)")
    parser.add_argument("--output", required=True, help="Output binary file path")
    args = parser.parse_args()

    start = int(args.start, 16)
    length = int(args.length)
    if length <= 0:
        raise RuntimeError("length must be > 0")

    port = find_port(args.port)
    fd = os.open(port, os.O_RDWR | os.O_NOCTTY | os.O_NONBLOCK)
    try:
        configure_serial(fd)
        _ = read_lines(fd, 0.25)

        cmd = f"READ|{start:05X}|{length}"
        os.write(fd, (cmd + "\n").encode("ascii"))

        payload = bytearray()
        done = False
        deadline = time.time() + 8.0

        while time.time() < deadline and not done:
            for line in read_lines(fd, 0.30):
                if line.startswith("ERR|"):
                    raise RuntimeError(f"device error: {line}")
                if line.startswith("OK|READ|"):
                    done = True
                    break
                frame = parse_data_frame(line)
                if frame is None:
                    continue
                _, _, data = frame
                payload.extend(data)
                if len(payload) >= length:
                    payload = payload[:length]

        if not done:
            raise RuntimeError("timed out waiting for READ completion")
        if len(payload) != length:
            raise RuntimeError(f"incomplete dump: expected {length} bytes, got {len(payload)}")

        os.makedirs(os.path.dirname(args.output), exist_ok=True)
        with open(args.output, "wb") as f:
            f.write(payload)

        print(f"PORT={port}")
        print(f"WROTE={args.output}")
        print(f"SIZE={len(payload)}")
        return 0
    finally:
        os.close(fd)


if __name__ == "__main__":
    try:
        raise SystemExit(run())
    except Exception as exc:  # noqa: BLE001
        print(f"ERROR: {exc}", file=sys.stderr)
        raise SystemExit(1)
