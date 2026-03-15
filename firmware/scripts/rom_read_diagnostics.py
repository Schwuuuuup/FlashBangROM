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


def pick_port(explicit: str | None) -> str:
    if explicit:
        return explicit
    ports = sorted(glob.glob('/dev/ttyACM*'))
    if not ports:
        raise RuntimeError('no /dev/ttyACM* device found')
    return ports[-1]


def read_lines(fd: int, timeout_s: float) -> list[str]:
    end = time.time() + timeout_s
    buf = b''
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
        while b'\n' in buf:
            line, buf = buf.split(b'\n', 1)
            text = line.replace(b'\r', b'').decode('utf-8', errors='replace').strip()
            if text:
                lines.append(text)
    return lines


def send_cmd(fd: int, cmd: str, timeout_s: float = 1.2) -> list[str]:
    os.write(fd, (cmd + '\n').encode('ascii'))
    return read_lines(fd, timeout_s)


def parse_id(lines: list[str]) -> tuple[int, int] | None:
    for line in lines:
        if not line.startswith('OK|ID|'):
            continue
        detail = line[len('OK|ID|'):]
        fields = {}
        for token in detail.split(','):
            if '=' not in token:
                continue
            k, v = token.split('=', 1)
            fields[k.strip()] = v.strip()
        if 'mf' not in fields or 'dev' not in fields:
            continue
        try:
            mf = int(fields['mf'].lower().replace('0x', ''), 16)
            dev = int(fields['dev'].lower().replace('0x', ''), 16)
            return (mf, dev)
        except ValueError:
            continue
    return None


def parse_data(lines: list[str]) -> bytes:
    payload = bytearray()
    for line in lines:
        if not line.startswith('DATA|'):
            continue
        parts = line.split('|', 3)
        if len(parts) != 4:
            continue
        payload.extend(bytes.fromhex(parts[3]))
    return bytes(payload)


def entropy_like(data: bytes) -> int:
    return len(set(data))


def main() -> int:
    parser = argparse.ArgumentParser(description='Read-only diagnostics for ROM wiring/protocol behavior')
    parser.add_argument('--port', help='Serial port to use (default: latest /dev/ttyACM*)')
    parser.add_argument('--id-samples', type=int, default=8, help='Number of ID samples (default: 8)')
    args = parser.parse_args()

    port = pick_port(args.port)
    fd = os.open(port, os.O_RDWR | os.O_NOCTTY | os.O_NONBLOCK)
    try:
        configure_serial(fd)
        _ = read_lines(fd, 0.25)

        print(f'PORT={port}')
        print('=== ID Samples ===')
        id_values: list[tuple[int, int] | None] = []
        for i in range(args.id_samples):
            lines = send_cmd(fd, 'ID', 1.0)
            parsed = parse_id(lines)
            id_values.append(parsed)
            print(f'  #{i + 1}: {parsed} raw={"; ".join(lines) if lines else "<none>"}')

        print('=== Single-Byte Probes ===')
        probe_addrs = [0x00000, 0x00001, 0x00002, 0x00055, 0x02AAA, 0x05555, 0x07FFF, 0x10000]
        probe_samples = 10
        probe_results: dict[int, list[int]] = {}
        for addr in probe_addrs:
            vals: list[int] = []
            for _ in range(probe_samples):
                lines = send_cmd(fd, f'READ|{addr:05X}|1', 0.9)
                data = parse_data(lines)
                vals.append(data[0] if data else -1)
            probe_results[addr] = vals
            uniq = sorted({v for v in vals if v >= 0})
            print(f'  0x{addr:05X}: samples={vals} unique={uniq}')

        print('=== Block Stability ===')
        block_addr = 0x00000
        block_len = 256
        blocks = []
        for i in range(3):
            lines = send_cmd(fd, f'READ|{block_addr:05X}|{block_len}', 1.5)
            data = parse_data(lines)
            blocks.append(data)
            print(f'  read#{i + 1}: len={len(data)} unique-bytes={entropy_like(data)}')
        same = len(blocks) == 3 and blocks[0] == blocks[1] == blocks[2]
        print(f'  all-identical={same}')

        print('=== Heuristic Summary ===')
        parsed_ok = [x for x in id_values if x is not None]
        known_ids = {
            (0xBF, 0xB5): 'SST39SF010A',
            (0xBF, 0xB6): 'SST39SF020A',
            (0xBF, 0xB7): 'SST39SF040',
        }
        observed_known = [known_ids[x] for x in parsed_ok if x in known_ids]
        if observed_known:
            uniq = sorted(set(observed_known))
            print(f'  status: known ID observed: {", ".join(uniq)}')
        else:
            print('  status: no known SST39 ID observed')

        all_probe = [v for vals in probe_results.values() for v in vals if v >= 0]
        if all_probe and len(set(all_probe)) == 1:
            print(f'  warning: probe bytes are constant (0x{all_probe[0]:02X}) across addresses; likely stuck/floating bus')

        if same and blocks[0]:
            ub = entropy_like(blocks[0])
            print(f'  note: first 256-byte block stable across reads (unique-bytes={ub})')

        return 0
    finally:
        os.close(fd)


if __name__ == '__main__':
    try:
        raise SystemExit(main())
    except Exception as exc:  # noqa: BLE001
        print(f'ERROR: {exc}', file=sys.stderr)
        raise SystemExit(1)
