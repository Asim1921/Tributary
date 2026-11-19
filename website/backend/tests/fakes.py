"""The in-memory Repository and Cache from app.memory, with a switch to simulate an outage."""
from app.memory import MemoryCache, MemoryRepository


class Down(Exception):
    pass


def _guarded(base, label: str, names: list[str]):
    """Subclass `base` so each named coroutine method raises Down while `self.down` is set."""
    ns: dict = {"down": False}

    def make(name: str):
        original = getattr(base, name)

        async def method(self, *a, **kw):
            if self.down:
                raise Down(label)
            return await original(self, *a, **kw)

        method.__name__ = name
        return method

    for n in names:
        ns[n] = make(n)
    return type("Fake" + base.__name__[6:], (base,), ns)


FakeRepo = _guarded(
    MemoryRepository, "mongo down",
    ["ping", "ensure_indexes", "upsert_release", "list_releases", "get_release",
     "record_download", "download_counts"],
)
FakeCache = _guarded(
    MemoryCache, "redis down",
    ["ping", "get", "set", "delete", "set_once", "counters", "init_counters", "incr"],
)
