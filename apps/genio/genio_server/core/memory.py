"""Session memory for Genio — SQLite, survives daemon restarts.

Two kinds of state live here:

* **Working memory** — the transcript of the current conversation plus the tool
  results the agent has observed. Truncated by a character budget before it is
  sent back to the model, so context cannot grow without bound.
* **Pending actions** — when an R2 action pauses for human approval, the exact
  tool + arguments are persisted, so the decision survives a restart and the
  loop can resume deterministically.

Everything lives under ``<GENIO_STATE_DIR>/memory/genio.db`` (WAL, 0700 dir).
No tokens are stored — only conversation, observations and pending intents.
"""

from __future__ import annotations

import json
import os
import sqlite3
import threading
import time
from pathlib import Path
from typing import Any, Iterable

SCHEMA = """
CREATE TABLE IF NOT EXISTS sessions (
    id          TEXT PRIMARY KEY,
    created_at  REAL NOT NULL,
    updated_at  REAL NOT NULL,
    title       TEXT NOT NULL DEFAULT ''
);
CREATE TABLE IF NOT EXISTS messages (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    session_id  TEXT NOT NULL,
    role        TEXT NOT NULL,
    content     TEXT NOT NULL,
    meta        TEXT,
    ts          REAL NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_messages_session ON messages(session_id, id);
CREATE TABLE IF NOT EXISTS tool_results (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    session_id  TEXT NOT NULL,
    tool        TEXT NOT NULL,
    args        TEXT NOT NULL,
    decision    TEXT,
    ok          INTEGER NOT NULL,
    result      TEXT,
    ts          REAL NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_tool_results_session ON tool_results(session_id, id);
CREATE TABLE IF NOT EXISTS pending_actions (
    session_id  TEXT PRIMARY KEY,
    tool        TEXT NOT NULL,
    args        TEXT NOT NULL,
    approval_id TEXT,
    step        INTEGER,
    created_at  REAL NOT NULL
);
"""

MAX_CONTENT = 8000


class MemoryStore:
    """Thread-safe, restart-durable session store."""

    def __init__(self, path: Path | str) -> None:
        self.path = Path(path)
        if self.path.suffix != ".db":
            self.path = self.path / "genio.db"
        self.path.parent.mkdir(parents=True, exist_ok=True, mode=0o700)
        self._lock = threading.RLock()
        self._db = sqlite3.connect(str(self.path), check_same_thread=False, timeout=10)
        self._db.row_factory = sqlite3.Row
        with self._lock:
            self._db.execute("PRAGMA journal_mode=WAL")
            self._db.execute("PRAGMA synchronous=NORMAL")
            self._db.executescript(SCHEMA)
            self._db.commit()

    # ---------------------------------------------------------------- sessions
    def ensure_session(self, session_id: str, title: str = "") -> None:
        now = time.time()
        with self._lock:
            self._db.execute(
                "INSERT INTO sessions(id, created_at, updated_at, title) VALUES(?,?,?,?) "
                "ON CONFLICT(id) DO UPDATE SET updated_at=excluded.updated_at, "
                "title=CASE WHEN excluded.title != '' THEN excluded.title ELSE sessions.title END",
                (session_id, now, now, (title or "")[:120]),
            )
            self._db.commit()

    def sessions(self, limit: int = 20) -> list[dict[str, Any]]:
        with self._lock:
            rows = self._db.execute(
                "SELECT s.id, s.title, s.created_at, s.updated_at, "
                "  (SELECT COUNT(*) FROM messages m WHERE m.session_id = s.id) AS messages "
                "FROM sessions s ORDER BY s.updated_at DESC LIMIT ?", (limit,)
            ).fetchall()
        return [dict(r) for r in rows]

    # ---------------------------------------------------------------- messages
    def append_message(self, session_id: str, role: str, content: str,
                       meta: dict[str, Any] | None = None) -> int:
        if role not in ("user", "assistant", "system", "tool"):
            raise ValueError(f"invalid role {role!r}")
        now = time.time()
        text = (content or "")[:MAX_CONTENT]
        with self._lock:
            self.ensure_session(session_id)
            cur = self._db.execute(
                "INSERT INTO messages(session_id, role, content, meta, ts) VALUES(?,?,?,?,?)",
                (session_id, role, text, json.dumps(meta, ensure_ascii=False) if meta else None, now),
            )
            self._db.execute("UPDATE sessions SET updated_at=? WHERE id=?", (now, session_id))
            self._db.commit()
            return int(cur.lastrowid)

    def history(self, session_id: str, limit: int = 50,
                max_chars: int = 12000) -> list[dict[str, Any]]:
        """Newest-last history, truncated from the front to fit ``max_chars``."""
        with self._lock:
            rows = self._db.execute(
                "SELECT role, content, meta, ts FROM messages WHERE session_id=? "
                "ORDER BY id DESC LIMIT ?", (session_id, limit)
            ).fetchall()
        out: list[dict[str, Any]] = []
        used = 0
        for row in rows:  # newest → oldest
            content = row["content"]
            if used + len(content) > max_chars and out:
                break
            out.append({"role": row["role"], "content": content,
                        "ts": row["ts"],
                        "meta": json.loads(row["meta"]) if row["meta"] else None})
            used += len(content)
        out.reverse()  # back to chronological order
        return out

    # ------------------------------------------------------------ tool results
    def record_tool_result(self, session_id: str, tool: str, args: dict[str, Any],
                           decision: str, ok: bool, result: Any) -> None:
        with self._lock:
            self._db.execute(
                "INSERT INTO tool_results(session_id, tool, args, decision, ok, result, ts) "
                "VALUES(?,?,?,?,?,?,?)",
                (session_id, tool, json.dumps(args, ensure_ascii=False)[:MAX_CONTENT],
                 decision, 1 if ok else 0,
                 json.dumps(result, ensure_ascii=False)[:MAX_CONTENT], time.time()),
            )
            self._db.commit()

    def tool_results(self, session_id: str, limit: int = 20) -> list[dict[str, Any]]:
        with self._lock:
            rows = self._db.execute(
                "SELECT tool, args, decision, ok, result, ts FROM tool_results "
                "WHERE session_id=? ORDER BY id DESC LIMIT ?", (session_id, limit)
            ).fetchall()
        return [{"tool": r["tool"], "args": _loads(r["args"]),
                 "decision": r["decision"], "ok": bool(r["ok"]),
                 "result": _loads(r["result"]), "ts": r["ts"]} for r in reversed(rows)]

    # --------------------------------------------------------- pending actions
    def set_pending(self, session_id: str, tool: str, args: dict[str, Any],
                    approval_id: str | None, step: int) -> None:
        with self._lock:
            self._db.execute(
                "INSERT INTO pending_actions(session_id, tool, args, approval_id, step, created_at) "
                "VALUES(?,?,?,?,?,?) ON CONFLICT(session_id) DO UPDATE SET "
                "tool=excluded.tool, args=excluded.args, approval_id=excluded.approval_id, "
                "step=excluded.step, created_at=excluded.created_at",
                (session_id, tool, json.dumps(args, ensure_ascii=False),
                 approval_id, step, time.time()),
            )
            self._db.commit()

    def get_pending(self, session_id: str) -> dict[str, Any] | None:
        with self._lock:
            row = self._db.execute(
                "SELECT tool, args, approval_id, step, created_at FROM pending_actions "
                "WHERE session_id=?", (session_id,)
            ).fetchone()
        if not row:
            return None
        return {"tool": row["tool"], "args": _loads(row["args"]),
                "approval_id": row["approval_id"], "step": row["step"],
                "created_at": row["created_at"]}

    def clear_pending(self, session_id: str) -> None:
        with self._lock:
            self._db.execute("DELETE FROM pending_actions WHERE session_id=?", (session_id,))
            self._db.commit()

    # ------------------------------------------------------------------ house
    def export(self, session_id: str, limit: int = 100) -> dict[str, Any]:
        return {
            "session_id": session_id,
            "messages": self.history(session_id, limit=limit, max_chars=10 ** 7),
            "tool_results": self.tool_results(session_id, limit=limit),
            "pending": self.get_pending(session_id),
        }

    def close(self) -> None:
        with self._lock:
            self._db.commit()
            self._db.close()

    def __enter__(self) -> "MemoryStore":
        return self

    def __exit__(self, *_: Iterable[Any]) -> None:
        self.close()


def _loads(raw: str | None) -> Any:
    if not raw:
        return None
    try:
        return json.loads(raw)
    except json.JSONDecodeError:
        return raw


def default_memory(state_dir: Path | str | None = None) -> MemoryStore:
    base = Path(state_dir or os.getenv("GENIO_STATE_DIR", "/var/lib/genio"))
    return MemoryStore(base / "memory")
