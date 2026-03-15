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
    ports = sorted(glob.glob('/dev/ttyACM*'))
    if not ports:
        raise RuntimeError('no /dev/ttyACM* device found')
    return ports[-1]


def read_lines(fd: int, timeout_s: float) -> list[str]:
    end = time.time() + timeout_s
    lines: list[str] = []
    buf = b''
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
        while b'\n' in buf:
            line, buf = buf.split(b'\n', 1)
            text = line.replace(b'\r', b'').decode('utf-8', errors='replace').strip()
            if text:
                lines.append(text)
    return lines


def send_cmd(fd: int, cmd: str, timeout_s: float) -> list[str]:
    os.write(fd, (cmd + '\n').encode('ascii'))
    return read_lines(fd, timeout_s)


def parse_read_frames(lines: list[str]) -> bytes:
    payload = bytearray()
    for line in lines:
        if not line.startswith('DATA|'):
            continue
        parts = line.split('|', 3)
        if len(parts) != 4:
            continue
        payload.extend(bytes.fromhex(parts[3]))
    return bytes(payload)


def run() -> int:
    parser = argparse.ArgumentParser(description='Program ROM range with incremental byte pattern')
    parser.add_argument('--port', help='Serial port (default: latest /dev/ttyACM*)')
    parser.add_argument('--start', default='00000', help='Start address in hex (default: 00000)')
    parser.add_argument('--length', type=int, default=256, help='Length in bytes (default: 256)')
    parser.add_argument('--erase-sector', action='store_true', help='Erase sector at start address before programming')
    parser.add_argument('--cmd-timeout', type=float, default=0.8, help='Per-command response window in seconds')
    parser.add_argument('--retries', type=int, default=3, help='Retries for ID/program commands')
    parser.add_argument('--pace-ms', type=int, default=5, help='Delay between program commands in milliseconds')
    parser.add_argument('--output', required=True, help='Output readback binary path')
    args = parser.parse_args()

    start = int(args.start, 16)
    length = int(args.length)
    if length <= 0:
        raise RuntimeError('length must be > 0')

    port = find_port(args.port)
    fd = os.open(port, os.O_RDWR | os.O_NOCTTY | os.O_NONBLOCK)
    try:
        configure_serial(fd)
        _ = read_lines(fd, 0.50)

        # Ensure monitor mode is off before programming.
        _ = send_cmd(fd, 'DATA_BUS_MONITOR_STOP', 0.30)

        id_lines: list[str] = []
        for _ in range(max(1, args.retries)):
            id_lines = send_cmd(fd, 'ID', max(args.cmd_timeout, 1.0))
            if any('mf=0xbf' in line.lower() for line in id_lines):
                break
            time.sleep(0.05)
        if not any('mf=0xbf' in line.lower() for line in id_lines):
            raise RuntimeError(f'unsupported chip/not detected: {id_lines}')

        if args.erase_sector:
            erase_lines = send_cmd(fd, f'SECTOR_ERASE|{start:05X}', 2.50)
            if not any(line.startswith('OK|SECTOR_ERASE|') for line in erase_lines):
                raise RuntimeError(f'sector erase failed: {erase_lines}')

        for i in range(length):
            addr = start + i
            value = i & 0xFF
            ok = False
            lines: list[str] = []
            for _ in range(max(1, args.retries)):
                lines = send_cmd(fd, f'PROGRAM_BYTE|{addr:05X}|{value:02X}', args.cmd_timeout)
                if any(line.startswith('OK|PROGRAM_BYTE|') for line in lines):
                    ok = True
                    break
                time.sleep(0.02)
            if not ok:
                raise RuntimeError(f'program failed at 0x{addr:05X}: {lines}')
            if args.pace_ms > 0:
                time.sleep(args.pace_ms / 1000.0)

        os.write(fd, f'READ|{start:05X}|{length}\n'.encode('ascii'))
        all_lines: list[str] = []
        done = False
        deadline = time.time() + 8.0
        while time.time() < deadline and not done:
            lines = read_lines(fd, 0.35)
            all_lines.extend(lines)
            if any(line.startswith('ERR|') for line in lines):
                err = [line for line in lines if line.startswith('ERR|')][0]
                raise RuntimeError(f'readback error: {err}')
            if any(line.startswith('OK|READ|') for line in lines):
                done = True

        payload = parse_read_frames(all_lines)[:length]
        if len(payload) != length:
            raise RuntimeError(f'incomplete readback: expected {length}, got {len(payload)}')

        expected = bytes((i & 0xFF) for i in range(length))
        mismatches = sum(1 for a, b in zip(payload, expected) if a != b)

        os.makedirs(os.path.dirname(args.output), exist_ok=True)
        with open(args.output, 'wb') as f:
            f.write(payload)

        print(f'PORT={port}')
        print(f'WROTE={args.output}')
        print(f'SIZE={len(payload)}')
        print(f'MISMATCHES={mismatches}')
        print(f'FIRST16={payload[:16].hex()}')
        return 0
    finally:
        os.close(fd)


if __name__ == '__main__':
    try:
        raise SystemExit(run())
    except Exception as exc:  # noqa: BLE001
        print(f'ERROR: {exc}', file=sys.stderr)
        raise SystemExit(1)
