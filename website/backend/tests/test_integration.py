"""Run against the real stack: API_BASE_URL=http://localhost:8080 pytest -m integration"""
import os

import httpx
import pytest

pytestmark = pytest.mark.integration

BASE = os.environ.get("API_BASE_URL", "http://localhost:8080")


@pytest.fixture
def live():
    with httpx.Client(base_url=BASE, timeout=10) as c:
        try:
            c.get("/api/health")
        except httpx.HTTPError:
            pytest.skip(f"no stack at {BASE}")
        yield c


def test_health_and_latest(live):
    assert live.get("/api/health").json()["status"] == "ok"
    latest = live.get("/api/releases/latest").json()
    assert latest["version"] and len(latest["sha256"]) == 64


def test_download_returns_installer_with_manifest_checksum(live):
    import hashlib

    latest = live.get("/api/releases/latest").json()
    with live.stream("GET", "/api/download/latest", headers={"user-agent": "integration-test"}) as r:
        assert r.status_code == 200
        assert "attachment" in r.headers["content-disposition"]
        h = hashlib.sha256()
        for chunk in r.iter_bytes():
            h.update(chunk)
    assert h.hexdigest() == latest["sha256"]


def test_stats_shape(live):
    d = live.get("/api/stats").json()["downloads"]
    assert isinstance(d["total"], int) and isinstance(d["by_version"], dict)
