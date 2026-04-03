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


def send_line(fd: int, line: str) -> None:
    data = (line + "\n").encode("utf-8")
    sent = 0
    while sent < len(data):
        try:
            n = os.write(fd, data[sent:])
        except BlockingIOError:
            n = 0
        if n <= 0:
            # Give the CDC endpoint a chance to make progress.
            time.sleep(0.001)
            continue
        sent += n


def collect_until_expected(
    fd: int,
    max_window_s: float,
    expected_lines: int,
    quiet_after_expected_s: float = 0.20,
) -> list[str]:
    start = time.time()
    last_rx = start
    all_lines: list[str] = []

    while (time.time() - start) < max_window_s:
        lines = read_lines(fd, 0.05)
        if lines:
            all_lines.extend(lines)
            last_rx = time.time()
            continue

        # Do not exit early before the first/expected responses have had a chance to arrive.
        if len(all_lines) < expected_lines:
            continue

        # Once expected responses are reached, allow a short quiet tail and stop.
        if (time.time() - last_rx) >= quiet_after_expected_s:
            break

    return all_lines


def capture_inspect(fd: int, timeout_s: float) -> list[str]:
    send_line(fd, "INSPECT")
    print("[capture] CMD=INSPECT")

    captured: list[str] = []
    start = time.time()
    while (time.time() - start) < timeout_s:
        lines = read_lines(fd, 0.2)
        if not lines:
            continue
        for line in lines:
            print(f"  RX={line}")
            captured.append(line)
            if line.startswith("OK|INSPECT"):
                return captured
    return captured


def filter_replay_lines(lines: list[str]) -> list[str]:
    # Keep only replayable payload lines from INSPECT output.
    return [line for line in lines if not line.startswith("#") and not line.startswith("OK")]


def run() -> int:
    parser = argparse.ArgumentParser(
        description="Capture INSPECT, replay non-comment/non-OK lines, and print responses"
    )
    parser.add_argument("--port", help="Serial port (default: latest /dev/ttyACM*)")
    parser.add_argument(
        "--capture-timeout",
        type=float,
        default=4.0,
        help="Seconds to wait for INSPECT output (default: 4.0)",
    )
    parser.add_argument(
        "--reply-window",
        type=float,
        default=0.35,
        help="Seconds to collect responses per replayed line (default: 0.35)",
    )
    parser.add_argument(
        "--inter-line-delay",
        type=float,
        default=0.02,
        help="Delay between replay lines in seconds (default: 0.02)",
    )
    parser.add_argument(
        "--burst",
        action="store_true",
        help="Send replay lines unthrottled in one burst and collect responses afterwards",
    )
    parser.add_argument(
        "--burst-window",
        type=float,
        default=2.0,
        help="Response collection window after burst replay in seconds (default: 2.0)",
    )
    parser.add_argument(
        "--expect-lines",
        type=int,
        default=0,
        help="Expected number of burst response lines (default: replay line count)",
    )
    parser.add_argument(
        "--burst-rx-drain",
        action="store_true",
        help="During burst TX, continuously drain RX to avoid CDC backpressure",
    )
    args = parser.parse_args()

    ports = sorted(glob.glob("/dev/ttyACM*"))
    if not ports and not args.port:
        print("NO_PORT: no /dev/ttyACM* found")
        return 1

    port = args.port if args.port else ports[-1]
    fd = os.open(port, os.O_RDWR | os.O_NOCTTY | os.O_NONBLOCK)

    try:
        configure_serial(fd)
        _ = read_lines(fd, 0.3)

        print(f"PORT={port}")
        all_inspect_lines = capture_inspect(fd, args.capture_timeout)
        if not all_inspect_lines:
            print("ERROR: no INSPECT response captured")
            return 2

        replay_lines = filter_replay_lines(all_inspect_lines)
        if not replay_lines:
            print("ERROR: no replayable lines captured (all lines were # or OK)")
            return 3

        print(f"[replay] LINES={len(replay_lines)}")
        if args.burst:
            burst_live_rx: list[str] = []
            for idx, line in enumerate(replay_lines, 1):
                print(f"  TX[{idx}]={line}")
                send_line(fd, line)
                if args.burst_rx_drain:
                    burst_live_rx.extend(read_lines(fd, 0.001))

            expected = args.expect_lines if args.expect_lines > 0 else len(replay_lines)
            responses = burst_live_rx + collect_until_expected(fd, args.burst_window, expected)
            print(f"[replay-burst] RX_LINES={len(responses)}")
            if not responses:
                print("  RX=<none>")
            else:
                for resp in responses:
                    print(f"  RX={resp}")
                ok_count = sum(1 for r in responses if r.startswith("OK|"))
                err_count = sum(1 for r in responses if r.startswith("ERR|"))
                other_count = len(responses) - ok_count - err_count
                print(f"[replay-burst] SUMMARY|OK={ok_count}|ERR={err_count}|OTHER={other_count}|EXPECTED={expected}")
        else:
            for idx, line in enumerate(replay_lines, 1):
                print(f"  TX[{idx}]={line}")
                send_line(fd, line)
                time.sleep(args.inter_line_delay)
                responses = read_lines(fd, args.reply_window)
                if not responses:
                    print("    RX=<none>")
                    continue
                for resp in responses:
                    print(f"    RX={resp}")

    finally:
        os.close(fd)

    return 0


if __name__ == "__main__":
    raise SystemExit(run())
