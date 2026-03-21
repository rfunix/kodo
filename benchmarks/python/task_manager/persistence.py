import json
from pathlib import Path

from .models import Task


def save_tasks(tasks: dict[int, Task], filepath: str) -> None:
    """Serialize tasks dict to a JSON file."""
    data = {str(k): v.model_dump() for k, v in tasks.items()}
    Path(filepath).write_text(json.dumps(data, indent=2), encoding="utf-8")


def load_tasks(filepath: str) -> dict[int, Task]:
    """Deserialize tasks dict from a JSON file."""
    path = Path(filepath)
    if not path.exists():
        return {}
    raw = json.loads(path.read_text(encoding="utf-8"))
    return {int(k): Task.model_validate(v) for k, v in raw.items()}
