"""In-memory Repository and Cache, used when no MongoDB / Redis is configured (and by the tests).

State lives in the process: on Vercel that is one function instance, so counts and cached values
last only as long as that instance does.
"""
import time
from datetime import datetime

from .models import Release, version_key


class MemoryRepository:
    def __init__(self) -> None:
        self.releases: dict[str, Release] = {}
        self.downloads: list[dict] = []
        self.indexes: list[str] = []
        self.list_calls = 0

    async def ping(self) -> None:
        pass

    async def ensure_indexes(self) -> None:
        self.indexes = ["releases.version unique", "downloads.at"]

    async def upsert_release(self, release: Release) -> None:
        self.releases[release.version] = release  # keyed by version: unique index semantics

    async def list_releases(self) -> list[Release]:
        self.list_calls += 1
        return sorted(self.releases.values(), key=lambda r: version_key(r.version), reverse=True)

    async def get_release(self, version: str) -> Release | None:
        return self.releases.get(version)

    async def record_download(self, version: str, at: datetime, ua_family: str) -> None:
        self.downloads.append({"version": version, "at": at, "ua_family": ua_family})

    async def download_counts(self) -> dict[str, int]:
        out: dict[str, int] = {}
        for d in self.downloads:
            out[d["version"]] = out.get(d["version"], 0) + 1
        return out

    async def close(self) -> None:
        pass


class MemoryCache:
    def __init__(self) -> None:
        self.kv: dict[str, str] = {}
        self.ttls: dict[str, int] = {}
        self.expires: dict[str, float] = {}
        self.hash: dict[str, int] = {}

    def _store(self, key: str, value: str, ttl: int) -> None:
        self.kv[key] = value
        self.ttls[key] = ttl
        self.expires[key] = time.monotonic() + ttl

    def _live(self, key: str) -> bool:
        if key in self.kv and self.expires.get(key, float("inf")) <= time.monotonic():
            self._drop(key)
        return key in self.kv

    def _drop(self, key: str) -> None:
        self.kv.pop(key, None)
        self.ttls.pop(key, None)
        self.expires.pop(key, None)

    async def ping(self) -> None:
        pass

    async def get(self, key: str) -> str | None:
        return self.kv[key] if self._live(key) else None

    async def set(self, key: str, value: str, ttl: int) -> None:
        if ttl > 0:
            self._store(key, value, ttl)

    async def delete(self, *keys: str) -> None:
        for k in keys:
            self._drop(k)

    async def set_once(self, key: str, ttl: int) -> bool:
        if self._live(key):
            return False
        self._store(key, "1", ttl)
        return True

    async def counters(self) -> dict[str, int] | None:
        return dict(self.hash) if "total" in self.hash else None

    async def init_counters(self, values: dict[str, int]) -> None:
        for k, v in values.items():
            self.hash.setdefault(k, v)

    async def incr(self, version: str) -> None:
        self.hash[version] = self.hash.get(version, 0) + 1
        self.hash["total"] = self.hash.get("total", 0) + 1

    async def close(self) -> None:
        pass

    def flush(self) -> None:
        """Simulate Redis restarting empty or evicting everything."""
        self.kv.clear()
        self.ttls.clear()
        self.expires.clear()
        self.hash.clear()
