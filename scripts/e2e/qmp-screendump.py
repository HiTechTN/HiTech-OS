#!/usr/bin/env python3
"""Ask a running QEMU monitor for a framebuffer dump.

Usage:
    ./scripts/e2e/qmp-screendump.py /tmp/qmp.sock /tmp/screen.ppm

QEMU must have been started with:
    -qmp unix:/tmp/qmp.sock,server,nowait
"""
import json
import socket
import sys


def read_message(stream):
    while True:
        line = stream.readline()
        if not line:
            raise RuntimeError("QMP connection closed")
        line = line.strip()
        if not line:
            continue
        try:
            return json.loads(line)
        except json.JSONDecodeError:
            continue


def command(stream, payload):
    stream.write((json.dumps(payload) + "\n").encode())
    while True:
        msg = read_message(stream)
        if "return" in msg or "error" in msg:
            return msg


def main():
    if len(sys.argv) != 3:
        print(__doc__.strip(), file=sys.stderr)
        return 2

    sock_path, output = sys.argv[1], sys.argv[2]
    sock = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
    sock.connect(sock_path)
    stream = sock.makefile("rwb", buffering=0)
    stream.readline()  # QMP greeting
    command(stream, {"execute": "qmp_capabilities"})
    result = command(stream, {"execute": "screendump", "arguments": {"filename": output}})
    sock.close()
    print(json.dumps(result))
    return 0 if "return" in result else 1


if __name__ == "__main__":
    raise SystemExit(main())
