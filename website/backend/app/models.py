from pydantic import BaseModel, Field

VERSION_PATTERN = r"^\d+\.\d+\.\d+$"
FILE_PATTERN = r"^[A-Za-z0-9][A-Za-z0-9._-]*$"


def version_key(version: str) -> tuple[int, int, int]:
    a, b, c = version.split(".")
    return int(a), int(b), int(c)


class Release(BaseModel):
    version: str = Field(pattern=VERSION_PATTERN)
    file: str = Field(pattern=FILE_PATTERN, max_length=128)
    size: int = Field(gt=0)
    sha256: str = Field(pattern=r"^[0-9a-f]{64}$")
    published: str = Field(pattern=r"^\d{4}-\d{2}-\d{2}$")
    notes: list[str] = []
    requirements: list[str] = []
    # The updater's minisign signature of the installer (the text of its .sig file). A release
    # without one is never offered to the app's updater.
    signature: str | None = Field(None, pattern=r"^[A-Za-z0-9+/=]+$", max_length=2048)


class Manifest(BaseModel):
    releases: list[Release]
