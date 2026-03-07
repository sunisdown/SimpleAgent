from __future__ import annotations

from dataclasses import dataclass
from typing import Any


@dataclass(slots=True)
class Event:
    type: str
    payload: dict[str, Any]


def make_event(event_type: str, **payload: Any) -> Event:
    return Event(type=event_type, payload=payload)

