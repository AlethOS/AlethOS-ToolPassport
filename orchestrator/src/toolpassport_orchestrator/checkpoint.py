"""Persistent LangGraph checkpoint helpers."""

from __future__ import annotations

import os
import sqlite3
from collections.abc import Iterator
from contextlib import contextmanager
from pathlib import Path

from langchain_core.runnables import RunnableConfig
from langgraph.checkpoint.sqlite import SqliteSaver


def checkpoint_config(run_id: str) -> RunnableConfig:
    """Bind a LangGraph thread to the authoritative Run ID."""
    return {"configurable": {"thread_id": run_id}}


@contextmanager
def sqlite_checkpointer(database_path: str) -> Iterator[SqliteSaver]:
    """Open a persistent SQLite checkpointer and close it after graph execution."""
    os.environ.setdefault("LANGGRAPH_STRICT_MSGPACK", "true")
    path = Path(database_path).expanduser()
    path.parent.mkdir(parents=True, exist_ok=True)
    connection = sqlite3.connect(path, check_same_thread=False)
    try:
        yield SqliteSaver(connection)
    finally:
        connection.close()


def has_checkpoint(checkpointer: SqliteSaver, config: RunnableConfig) -> bool:
    """Return whether this Run already has resumable graph state."""
    return checkpointer.get_tuple(config) is not None
