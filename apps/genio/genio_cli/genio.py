#!/usr/bin/env python3
"""Genio CLI — terminal client for the Genio daemon (Phase 2).

Thin client: no privileges, no secrets, no tool implementations. It talks to the
daemon over HTTP, renders telemetry with `rich`, and — critically — asks the
human (Y/N) when the daemon reports that an R2 action needs an approval token.

Usage
    python genio.py                      # interactive TUI
    python genio.py --once "status"      # one message, print reply, exit
    python genio.py --demo --yes         # scripted end-to-end demonstration
    python genio.py --url http://127.0.0.1:8765

Commands (interactive)
    status | tools | audit | approvals | call <tool> <json> | ask <text> | quit
"""

from __future__ import annotations

import argparse
import json
import sys
import urllib.error
import urllib.request
from typing import Any

from rich.console import Console
from rich.panel import Panel
from rich.prompt import Confirm
from rich.table import Table
from rich.text import Text

console = Console()
RISK_STYLE = {"R0": "green", "R1": "yellow", "R2": "bold red"}


class GenioClient:
    def __init__(self, base: str, timeout: float = 10.0) -> None:
        self.base = base.rstrip("/")
        self.timeout = timeout
        self.token: str | None = None
        self.caps: list[str] = []

    # -- transport ---------------------------------------------------------
    def _req(self, method: str, path: str, body: dict | None = None,
             headers: dict[str, str] | None = None,
             timeout: float | None = None) -> tuple[int, Any]:
        data = json.dumps(body).encode() if body is not None else None
        req = urllib.request.Request(self.base + path, data=data, method=method)
        req.add_header("Content-Type", "application/json")
        if self.token:
            req.add_header("X-Genio-Cap", self.token)
        for key, value in (headers or {}).items():
            req.add_header(key, value)
        try:
            with urllib.request.urlopen(req, timeout=timeout or self.timeout) as resp:
                return resp.status, json.loads(resp.read() or b"{}")
        except urllib.error.HTTPError as exc:
            try:
                return exc.code, json.loads(exc.read() or b"{}")
            except json.JSONDecodeError:
                return exc.code, {"detail": "non-JSON error"}
        except (urllib.error.URLError, TimeoutError) as exc:
            return 0, {"detail": f"daemon unreachable: {exc}"}

    # -- api ---------------------------------------------------------------
    def connect(self) -> bool:
        status, data = self._req("POST", "/caps/session", {"subject": "cli"})
        if status != 200:
            console.print(f"[red]session refused:[/] {data}")
            return False
        self.token = data["token"]
        self.caps = data.get("caps", [])
        status, health = self._req("GET", "/health")
        if status == 200:
            console.print(Panel.fit(
                f"[bold]Genio[/] v{health['version']} · daemon {health['status']} · "
                f"ticks {health['agent']['ticks']} · "
                f"tools {health['broker']['tools']} · "
                f"audit {'[green]writable[/]' if health['broker']['audit_writable'] else '[red]FROZEN[/]'}",
                title="connected", border_style="cyan"))
        return True

    def telemetry(self) -> dict[str, Any]:
        return self._req("GET", "/telemetry?fresh=true")[1]

    def tools(self) -> list[dict[str, Any]]:
        return self._req("GET", "/tools")[1].get("tools", [])

    def message(self, text: str) -> tuple[int, dict[str, Any]]:
        return self._req("POST", "/agent/message", {"message": text})

    def invoke(self, tool: str, args: dict[str, Any],
               approval_token: str | None = None) -> tuple[int, dict[str, Any]]:
        body: dict[str, Any] = {"args": args}
        if approval_token:
            body["approval_token"] = approval_token
        # R2 handlers may wait on systemd; allow more headroom than reads.
        return self._req("POST", f"/tools/{tool}/invoke", body, timeout=45.0)

    def grant(self, approval_id: str, approver: str = "cli-human") -> tuple[int, dict[str, Any]]:
        return self._req("POST", f"/approvals/{approval_id}/grant", {"approver": approver})

    def audit(self, n: int = 10) -> dict[str, Any]:
        return self._req("GET", f"/audit/tail?n={n}")[1]

    def audit_verify(self) -> dict[str, Any]:
        return self._req("GET", "/audit/verify")[1]


# --------------------------------------------------------------------------- render
def render_telemetry(snap: dict[str, Any]) -> None:
    if not snap:
        console.print("[red]no telemetry[/]")
        return
    host = snap.get("host") or {}
    cpu = snap.get("cpu") or {}
    mem = snap.get("memory") or {}
    svc = snap.get("services") or {}
    disks = snap.get("disks") or {}

    table = Table.grid(padding=(0, 2))
    table.add_column(style="bold cyan", no_wrap=True)
    table.add_column()
    table.add_row("host", f"{host.get('hostname')} · {host.get('kernel')} · "
                           f"gen {host.get('nixos_generation') or 'n/a'}")
    table.add_row("cpu", f"{cpu.get('percent')}% of {cpu.get('count')} cores")
    table.add_row("memory", f"{mem.get('used_mb')}/{mem.get('total_mb')} MB "
                            f"({mem.get('percent')}%)")
    table.add_row("services", f"[green]{svc.get('running_count')} running[/] · "
                              f"{svc.get('failed_count')} failed")
    if disks:
        table.add_row("disks", " · ".join(
            f"{mnt} {d['used_gb']}/{d['total_gb']}G ({d['percent']}%)"
            for mnt, d in disks.items()))
    table.add_row("uptime", f"{round((snap.get('uptime_s') or 0) / 3600, 1)} h")
    console.print(Panel(table, title="system telemetry", border_style="blue"))


def render_tools(tools: list[dict[str, Any]]) -> None:
    table = Table(title="tool catalogue", header_style="bold")
    table.add_column("tool", style="cyan")
    table.add_column("risk")
    table.add_column("cap")
    table.add_column("rate/min", justify="right")
    table.add_column("description")
    for tool in tools:
        risk = tool["risk"]
        table.add_row(tool["id"], Text(risk, style=RISK_STYLE.get(risk, "")),
                      tool["cap"], str(tool["rate_per_min"]), tool["description"])
    console.print(table)


def render_result(tool: str, data: dict[str, Any]) -> None:
    decision = data.get("decision")
    if data.get("decision") is None:
        console.print(Panel(f"{data.get('detail') or data}", title=f"[red]{tool} · transport error[/]",
                            border_style="red"))
    elif data.get("ok"):
        console.print(Panel(json.dumps(data.get("result"), indent=2, ensure_ascii=False)[:4000],
                            title=f"[green]{tool} · executed[/] (audit #{data.get('audit_seq')})",
                            border_style="green"))
    elif decision == "approval_required":
        console.print(Panel(data.get("message", ""), title="[yellow]approval required[/]",
                            border_style="yellow"))
    else:
        console.print(Panel(f"{data.get('code')}: {data.get('message')}",
                            title=f"[red]{tool} · {decision}[/]", border_style="red"))


# --------------------------------------------------------------------------- flows
def invoke_with_approval(client: GenioClient, tool: str, args: dict[str, Any],
                         auto_yes: bool = False) -> dict[str, Any]:
    """Invoke a tool; if R2, ask the human Y/N and retry with the approval token."""
    status, data = client.invoke(tool, args)
    if status != 409 or data.get("decision") != "approval_required":
        render_result(tool, data)
        return data

    console.print(Panel(
        f"[bold]{data.get('approval_prompt')}[/]\n\n"
        f"risk: [bold red]{data.get('risk')}[/] · tool: {tool}\n"
        f"args: {json.dumps(args, ensure_ascii=False)}\n"
        f"approval id: {data.get('approval_id')} · expires in {data.get('expires_in_s')}s",
        title="privileged action", border_style="yellow"))

    approved = True if auto_yes else Confirm.ask("Approve this action?", default=False)
    if not approved:
        console.print("[dim]denied by operator — nothing executed[/]")
        return {"ok": False, "decision": "denied", "code": "operator_denied"}

    status, granted = client.grant(data["approval_id"])
    if status != 200:
        console.print(f"[red]grant failed:[/] {granted}")
        return granted
    status, data = client.invoke(tool, args, approval_token=granted["approval_token"])
    render_result(tool, data)
    return data


def run_demo(client: GenioClient, auto_yes: bool) -> int:
    """Scripted proof: R0, R1, R2 (denied → approve → executed), audit chain."""
    console.rule("[bold]1 · telemetry (R0)")
    render_telemetry(client.telemetry())

    console.rule("[bold]2 · tool catalogue")
    tools = client.tools()
    render_tools(tools)

    console.rule("[bold]3 · R0 invoke: services.list")
    status, data = client.invoke("services.list", {})
    render_result("services.list", data)

    console.rule("[bold]4 · R1 invoke: file.write (audited)")
    status, data = client.invoke("file.write", {"path": "demo/hello.txt",
                                                "content": "genio phase2 demo\n"})
    render_result("file.write", data)

    console.rule("[bold]5 · R2 without approval (must be refused)")
    status, data = client.invoke("service.restart", {"unit": "cron.service"})
    render_result("service.restart", data)
    if data.get("decision") != "approval_required":
        console.print("[red]SECURITY FAILURE: R2 executed without approval[/]")
        return 1

    console.rule("[bold]6 · R2 with human approval (Y/N prompt)")
    invoke_with_approval(client, "service.restart", {"unit": "cron.service"}, auto_yes=auto_yes)

    console.rule("[bold]7 · audit chain")
    verify = client.audit_verify()
    console.print(verify)
    for record in client.audit(6).get("records", []):
        console.print(f"[dim]#{record.get('seq')}[/] {record.get('decision'):<20} "
                      f"{record.get('risk')} {record.get('tool')} → {record.get('detail')}")
    return 0 if verify.get("ok") else 1


def run_once(client: GenioClient, message: str) -> int:
    status, data = client.message(message)
    if status != 200:
        console.print(f"[red]{data}[/]")
        return 1
    console.print(Panel(data.get("reply", ""), title=f"genio ← {message!r}",
                        border_style="cyan"))
    suggestion = data.get("suggestion")
    if suggestion:
        console.print(f"[dim]suggested tool: {suggestion['tool']} {suggestion['args']}[/]")
    return 0


def interactive(client: GenioClient) -> int:
    console.print("[dim]Commands: status · tools · audit · approvals · "
                  "call <tool> <json> · ask <text> · quit[/]")
    while True:
        try:
            line = console.input("[bold cyan]genio>[/] ").strip()
        except (EOFError, KeyboardInterrupt):
            console.print()
            return 0
        if not line:
            continue
        cmd, _, rest = line.partition(" ")
        cmd = cmd.lower()

        if cmd in ("quit", "exit", "q"):
            return 0
        if cmd in ("status", "telemetry"):
            render_telemetry(client.telemetry())
        elif cmd == "tools":
            render_tools(client.tools())
        elif cmd == "approvals":
            console.print(client._req("GET", "/approvals")[1])
        elif cmd == "audit":
            verify = client.audit_verify()
            console.print(f"chain: {'[green]OK[/]' if verify.get('ok') else '[red]BROKEN[/]'} "
                          f"· {verify.get('records')} records")
            for record in client.audit(8).get("records", []):
                console.print(f"[dim]#{record.get('seq')}[/] {record.get('decision')} "
                              f"{record.get('tool')}")
        elif cmd == "call":
            tool, _, raw = rest.partition(" ")
            try:
                args = json.loads(raw) if raw.strip() else {}
            except json.JSONDecodeError as exc:
                console.print(f"[red]invalid JSON:[/] {exc}")
                continue
            invoke_with_approval(client, tool, args)
        elif cmd == "ask":
            run_once(client, rest)
        else:
            run_once(client, line)
    return 0


def main() -> int:
    parser = argparse.ArgumentParser(description="Genio CLI (Phase 2)")
    parser.add_argument("--url", default="http://127.0.0.1:8765")
    parser.add_argument("--once", metavar="MESSAGE", help="send one message and exit")
    parser.add_argument("--demo", action="store_true", help="scripted end-to-end demo")
    parser.add_argument("--yes", action="store_true", help="auto-approve R2 (demo only)")
    args = parser.parse_args()

    client = GenioClient(args.url)
    if not client.connect():
        return 2
    if args.demo:
        return run_demo(client, args.yes)
    if args.once:
        return run_once(client, args.once)
    return interactive(client)


if __name__ == "__main__":
    sys.exit(main())
