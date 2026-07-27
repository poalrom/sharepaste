Desktop app for macOS (Apple Silicon) and Windows (x64). The relay is not
released as a binary — operators build it from `docker compose`.

**These bundles are not signed or notarized.** macOS quarantines anything a
browser downloaded, so a fresh install needs one command before it will launch:

```sh
xattr -dr com.apple.quarantine /Applications/sharepaste.app
```

That is a first-install tax, not a per-release one: updates taken from inside
the app are fetched by the app's own HTTP client, which never sets the
quarantine attribute, and are verified against a signing key compiled into the
binary. The reasoning is in
[ADR 0005](https://github.com/poalrom/sharepaste/blob/main/docs/adr/0005-unsigned-downloads-signed-updates.md).

The macOS build is Apple Silicon only — there is no Intel bundle.

---
