from pathlib import Path

from pydantic import AliasChoices, Field, field_validator
from pydantic_settings import BaseSettings, SettingsConfigDict


class Settings(BaseSettings):
    model_config = SettingsConfigDict(env_file=None, extra="ignore", populate_by_name=True)

    # MONGO_URL wins; MONGODB_URI is what the Vercel Marketplace MongoDB Atlas integration sets.
    mongo_url: str = Field("mongodb://mongo:27017", validation_alias=AliasChoices("MONGO_URL", "MONGODB_URI"))
    mongo_db: str = "tributary"
    mongo_timeout_ms: int | None = Field(None, gt=0)  # default 2000 (5000 in serverless mode)
    # REDIS_URL wins; KV_URL is what older Upstash KV integrations set. rediss:// (TLS) works.
    redis_url: str = Field("redis://redis:6379/0", validation_alias=AliasChoices("REDIS_URL", "KV_URL"))
    downloads_dir: str = "/srv/downloads"
    # Installer hosted elsewhere (Vercel Blob, any HTTPS folder): downloads answer 302 to
    # DOWNLOAD_BASE_URL/<file>. Unset keeps the X-Accel-Redirect hand-off to nginx.
    download_base_url: str | None = None
    # Release manifest. Default: downloads_dir/releases.json, or backend/releases.json when
    # DOWNLOAD_BASE_URL is set (the API project on Vercel only sees the backend folder).
    releases_file: str | None = None
    # True: connect and seed lazily on first request, no startup wait or background retries
    # (serverless). Default: on when DOWNLOAD_BASE_URL is set or running on Vercel (VERCEL=1).
    serverless: bool | None = None
    vercel: str | None = None
    count_salt: str = "change-me"
    cache_ttl_releases: int = Field(60, ge=0)
    cache_ttl_stats: int = Field(30, ge=0)
    cache_ttl_seen: int = Field(86400, ge=1)
    # Startup: how long to wait for the data services before serving anyway
    # (the app keeps retrying in the background; /api/health says 503 until ready).
    startup_wait_seconds: float = 60.0
    startup_backoff_initial: float = 0.5
    startup_backoff_max: float = 5.0
    log_level: str = "INFO"

    @field_validator("download_base_url", "releases_file", "serverless", "mongo_timeout_ms", mode="before")
    @classmethod
    def _blank_is_unset(cls, v):
        return None if isinstance(v, str) and not v.strip() else v

    @field_validator("download_base_url")
    @classmethod
    def _check_base_url(cls, v: str | None) -> str | None:
        if v is None:
            return None
        v = v.strip().rstrip("/")
        if not v.startswith(("https://", "http://")):
            raise ValueError("DOWNLOAD_BASE_URL must start with https:// (or http://)")
        return v

    @property
    def mongo_configured(self) -> bool:
        """True only when MONGO_URL / MONGODB_URI was actually provided (not the compose default)."""
        return "mongo_url" in self.model_fields_set and bool(self.mongo_url.strip())

    @property
    def redis_configured(self) -> bool:
        return "redis_url" in self.model_fields_set and bool(self.redis_url.strip())

    @property
    def lazy_startup(self) -> bool:
        if self.serverless is not None:
            return self.serverless
        return self.download_base_url is not None or bool(self.vercel)

    @property
    def manifest_path(self) -> Path:
        if self.releases_file:
            return Path(self.releases_file)
        if self.download_base_url:
            return Path(__file__).resolve().parent.parent / "releases.json"
        return Path(self.downloads_dir) / "releases.json"
