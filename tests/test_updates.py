"""1.x's update check: it reads the release list and offers only 1.x releases (2.x is skipped).

The release lists are recorded by hand (no network). The installed version is set per test."""
import urllib.request

import pytest

from stleditor import updates as U


class FakeSettings:
    def __init__(self):
        self.data = {"update_repo": "owner/repo", "update_check": True}

    def get(self, key, default=None):
        return self.data.get(key, default)

    def set(self, key, value):
        self.data[key] = value


def rel(tag, prerelease=False, draft=False):
    return {"tag_name": tag, "html_url": f"https://github.com/owner/repo/releases/tag/{tag}",
            "body": "notes", "prerelease": prerelease, "draft": draft}


@pytest.fixture
def installed(monkeypatch):
    """Pretend the installed 1.x is 1.13.0 and give the update check its own settings and state."""
    monkeypatch.setattr(U, "__version__", "1.13.0")
    monkeypatch.setattr(U, "settings", FakeSettings())
    monkeypatch.setattr(U, "STATE", dict(U.STATE))

    def use(releases):
        monkeypatch.setattr(U, "_releases", lambda timeout: releases)
        return U.check(force=True)
    return use


def test_2x_release_is_not_offered(installed):
    state = installed([rel("v2.0.0"), rel("v1.13.0")])
    assert state["latest"] == "1.13.0"
    assert U.status()["available"] is False


def test_newer_1x_still_offered_when_2x_is_newer(installed):
    state = installed([rel("v2.0.0"), rel("v1.14.0"), rel("v1.13.0")])
    assert state["latest"] == "1.14.0"
    assert U.status()["available"] is True


def test_only_2x_releases_offers_nothing(installed):
    state = installed([rel("v2.0.0"), rel("v2.0.1")])
    assert not state["latest"]
    assert U.status()["available"] is False


def test_drafts_and_prereleases_are_ignored(installed):
    state = installed([rel("v1.15.0", draft=True), rel("v1.14.1", prerelease=True), rel("v1.13.2")])
    assert state["latest"] == "1.13.2"


def test_newest_1x_wins_regardless_of_list_order(installed):
    state = installed([rel("v1.12.0"), rel("v2.1.0"), rel("v1.14.0"), rel("v1.13.0")])
    assert state["latest"] == "1.14.0"


def test_release_with_only_name_is_read_as_its_tag(installed):
    named = {"name": "v1.14.0", "html_url": "https://github.com/owner/repo/releases/tag/v1.14.0",
             "body": "notes", "prerelease": False, "draft": False}
    state = installed([rel("v2.0.0"), named])
    assert state["latest"] == "1.14.0"
    assert U.status()["available"] is True


def test_latest_1x_helper_returns_none_without_1x():
    assert U._latest_1x([rel("v2.0.0")]) is None
    assert U._latest_1x([]) is None


def test_release_list_is_fetched_not_releases_latest(monkeypatch):
    seen = []

    def fake_urlopen(req, timeout=None):
        seen.append(req.full_url)
        raise OSError("offline in tests")

    monkeypatch.setattr(U, "settings", FakeSettings())
    monkeypatch.setattr(U, "STATE", dict(U.STATE))
    monkeypatch.setattr(urllib.request, "urlopen", fake_urlopen)
    U.check(force=True)
    assert seen == ["https://api.github.com/repos/owner/repo/releases?per_page=100"]
    assert U.STATE["error"]


def test_install_picks_the_same_1x_release(monkeypatch):
    monkeypatch.setattr(U, "_releases", lambda timeout: [rel("v2.0.0"), rel("v1.14.0"), rel("v1.13.0")])
    assert U._release()["tag_name"] == "v1.14.0"


def test_install_refuses_when_no_1x_release(monkeypatch):
    monkeypatch.setattr(U, "_releases", lambda timeout: [rel("v2.0.0")])
    with pytest.raises(RuntimeError):
        U._release()
