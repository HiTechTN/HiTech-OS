#!/usr/bin/env python3
"""Record a headless QEMU framebuffer to MP4 through the QMP monitor.

QEMU is started with `-display none -qmp unix:...`. This script polls
`screendump` at a fixed cadence, streams the PPM frames into ffmpeg over a
pipe, and produces a real-time H.264 recording of the whole lifecycle.

Usage:
    qmp_recorder.py --socket /path/qmp.sock --out video.mp4 \
        --fps 1 --stop-file /path/stop
"""
import argparse
import json
import os
import signal
import socket
import subprocess
import sys
import tempfile
import time


class QMP:
    def __init__(self, path):
        self.sock = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
        self.sock.connect(path)
        self.stream = self.sock.makefile("rwb", buffering=0)
        self.stream.readline()  # greeting
        self.command({"execute": "qmp_capabilities"})

    def command(self, payload):
        self.stream.write((json.dumps(payload) + "\n").encode())
        while True:
            line = self.stream.readline()
            if not line:
                raise RuntimeError("QMP connection closed")
            line = line.strip()
            if not line:
                continue
            try:
                msg = json.loads(line)
            except json.JSONDecodeError:
                continue
            if "return" in msg or "error" in msg:
                return msg

    def screendump(self, path):
        return self.command({"execute": "screendump", "arguments": {"filename": path}})


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--socket", required=True)
    parser.add_argument("--out", required=True)
    parser.add_argument("--fps", type=float, default=1.0)
    parser.add_argument("--stop-file", default="/tmp/hitech-recorder.stop")
    parser.add_argument("--max-seconds", type=float, default=0)
    args = parser.parse_args()

    ffmpeg = subprocess.Popen(
        [
            "ffmpeg", "-hide_banner", "-loglevel", "error", "-y",
            "-f", "image2pipe", "-vcodec", "ppm", "-framerate", str(args.fps),
            "-i", "-",
            "-c:v", "libx264", "-preset", "veryfast", "-crf", "26",
            "-pix_fmt", "yuv420p", "-movflags", "+faststart",
            args.out,
        ],
        stdin=subprocess.PIPE,
    )

    qmp = QMP(args.socket)
    frame_path = tempfile.mktemp(prefix="hitech-frame-", suffix=".ppm")
    started = time.time()
    interval = 1.0 / args.fps
    frames = 0
    stopping = {"flag": False}

    def stop(_signum, _frame):
        stopping["flag"] = True

    signal.signal(signal.SIGTERM, stop)
    signal.signal(signal.SIGINT, stop)

    try:
        while not stopping["flag"]:
            if os.path.exists(args.stop_file):
                break
            if args.max_seconds and time.time() - started >= args.max_seconds:
                break
            tick = time.time()
            try:
                qmp.screendump(frame_path)
                with open(frame_path, "rb") as handle:
                    data = handle.read()
                if data.startswith(b"P6"):
                    ffmpeg.stdin.write(data)
                    ffmpeg.stdin.flush()
                    frames += 1
            except (OSError, RuntimeError) as error:
                print(f"recorder: frame skipped: {error}", file=sys.stderr)
            elapsed = time.time() - tick
            time.sleep(max(0.0, interval - elapsed))
    finally:
        try:
            ffmpeg.stdin.close()
        except OSError:
            pass
        ffmpeg.wait(timeout=120)
        try:
            os.unlink(frame_path)
        except OSError:
            pass
        qmp.stream.close()
        qmp.sock.close()

    print(f"recorder: wrote {frames} frames to {args.out}")


if __name__ == "__main__":
    main()
