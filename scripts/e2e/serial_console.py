#!/usr/bin/env python3
"""Minimal expect-style driver for a QEMU unix serial socket.

The NetInstaller live ISO exposes an autologin shell on ttyS0. This helper
sends commands and waits for a unique marker so the E2E harness can drive the
graphical installer backend without a display server.
"""
import os
import re
import select
import socket
import time


class SerialConsole:
    def __init__(self, path, timeout=30):
        self.path = path
        self.sock = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
        self.sock.connect(path)
        self.sock.setblocking(False)
        self.buffer = ""
        self.timeout = timeout
        self.counter = 0

    def close(self):
        try:
            self.sock.close()
        except OSError:
            pass

    def read(self, timeout=1.0):
        deadline = time.time() + timeout
        chunks = []
        while time.time() < deadline:
            r, _, _ = select.select([self.sock], [], [], max(0, deadline - time.time()))
            if not r:
                break
            try:
                data = self.sock.recv(65536)
            except BlockingIOError:
                continue
            if not data:
                break
            chunks.append(data)
        text = b"".join(chunks).decode("utf-8", "replace")
        self.buffer += text
        return text

    def send(self, text):
        self.sock.sendall(text.encode())

    def sendline(self, line):
        self.send(line + "\n")

    def expect(self, pattern, timeout=None, drain=True):
        """Wait until `pattern` (regex) appears; return text since the call."""
        timeout = self.timeout if timeout is None else timeout
        regex = re.compile(pattern)
        deadline = time.time() + timeout
        start = len(self.buffer)
        while time.time() < deadline:
            match = regex.search(self.buffer)
            if match:
                out = self.buffer[start:]
                return out
            self.read(min(1.0, max(0.05, deadline - time.time())))
        out = self.buffer[start:]
        raise TimeoutError(f"timeout waiting for {pattern!r}; tail={out[-500:]!r}")

    def run(self, command, timeout=120, marker=None):
        """Run a shell command and return output up to its completion marker."""
        self.counter += 1
        marker = marker or f"__HITECH_DONE_{self.counter}__"
        self.sendline(f"{command}; echo {marker}")
        out = self.expect(re.escape(marker), timeout=timeout)
        # Strip the echoed command line and the marker itself.
        lines = out.splitlines()
        if lines and command.split("\n")[0][:40] in lines[0]:
            lines = lines[1:]
        text = "\n".join(lines).replace(marker, "")
        return text.strip()

    def wait_for_login(self, timeout=240):
        return self.expect(r"\[nixos@.*\][^\n]*[#$] ?$|login: ", timeout=timeout)


def main():
    import argparse
    parser = argparse.ArgumentParser()
    parser.add_argument("socket")
    parser.add_argument("command", nargs="?")
    parser.add_argument("--timeout", type=float, default=60)
    args = parser.parse_args()
    con = SerialConsole(args.socket)
    if args.command:
        print(con.run(args.command, timeout=args.timeout))
    else:
        print(con.read(timeout=10))
    con.close()


if __name__ == "__main__":
    main()
