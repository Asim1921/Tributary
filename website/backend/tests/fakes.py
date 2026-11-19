"""In-memory implementations of the Repository and Cache ports, with a switch to simulate an outage."""
from datetime import datetime

from app.models import Release, version_key


class Down(Exception):
    pass


class FakeRepo:
    def __init__(self) -> None:
        self.releases: dict[str, Release] = {}
        self.downloads: list[dict] = []
        self.indexes: list[str] = []
        self.down = False
        self.list_calls = 0

    def _check(self) -> None:
        if self.down:
            raise Down("mongo down")

    async def ping(self) -> None:
        self._check()

    async def ensure_indexes(self) -> None:
        self._check()
        self.indexes = ["releases.version unique", "downloads.at"]

    async def upsert_release(self, release: Release) -> None:
        self._check()
        self.releases[release.version] = release  # keyed by version: unique index semantics

    async def list_releases(self) -> list[Release]:
        self._check()
        self.list_calls += 1
        return sorted(self.releases.values(), key=lambda r: version_key(r.version), reverse=True)

    async def get_release(self, version: str) -> Release | None:
        self._check()
        return self.releases.get(version)

    async def record_download(self, version: str, at: datetime, ua_family: str) -> None:
        self._check()
        self.downloads.append({"version": version, "at": at, "ua_family": ua_family})

    async def download_counts(self) -> dict[str, int]:
        self._check()
        out: dict[str, int] = {}
        for d in self.downloads:
            out[d["version"]] = out.get(d["version"], 0) + 1
        return out

    async def close(self) -> None:
        pass


class FakeCache:
    def __init__(self) -> None:
        self.kv: dict[str, str] = {}
        self.ttls: dict[str, int] = {}
        self.hash: dict[str, int] = {}
        self.down = False

    def _check(self) -> None:
        if self.down:
            raise Down("redis down")

    async def ping(self) -> None:
        self._check()

    async def get(self, key: str) -> str | None:
        self._check()
        return self.kv.get(key)

    async def set(self, key: str, value: str, ttl: int) -> None:
        self._check()
        if ttl > 0:
            self.kv[key] = value
            self.ttls[key] = ttl

    async def delete(self, *keys: str) -> None:
        self._check()
        for k in keys:
            self.kv.pop(k, None)
            self.ttls.pop(k, None)

    async def set_once(self, key: str, ttl: int) -> bool:
        self._check()
        if key in self.kv:
            return False
        self.kv[key] = "1"
        self.ttls[key] = ttl
        return True

    async def counters(self) -> dict[str, int] | None:
        self._check()
        return dict(self.hash) if "total" in self.hash else None

    async def init_counters(self, values: dict[str, int]) -> None:
        self._check()
        for k, v in values.items():
            self.hash.setdefault(k, v)

    async def incr(self, version: str) -> None:
        self._check()
        self.hash[version] = self.hash.get(version, 0) + 1
        self.hash["total"] = self.hash.get("total", 0) + 1

    async def close(self) -> None:
        pass

    def flush(self) -> None:
        """Simulate Redis restarting empty or evicting everything."""
        self.kv.clear()
        self.ttls.clear()
        self.hash.clear()
