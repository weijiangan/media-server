Packaging notes
================

Build (Arch Linux / AUR-like):

1. Place this repository in a build directory and run `makepkg -si` in the repo root (where `PKGBUILD` lives) or copy `PKGBUILD` into a separate build directory and run `makepkg -si` there.

2. The PKGBUILD builds the `server` crate in release mode and installs the binary to `/usr/bin/media-server`.

Service (systemd):

1. The package installs `media-server.service` to `/usr/lib/systemd/system/`.
2. Create a `media` system user or run the service as `root` (not recommended):

```sh
sudo useradd --system --create-home --home-dir /var/lib/media-server --shell /usr/sbin/nologin media
sudo mkdir -p /var/lib/media-server /var/log/media-server
sudo chown -R media:media /var/lib/media-server /var/log/media-server
sudo systemctl daemon-reload
sudo systemctl enable --now media-server.service
```

Configuration
-------------
The packaged `config.json` is installed to `/etc/media-server/config.json`. Edit that file to point `directory_to_scan`, `db_path`, and `thumbnails_dir` to appropriate paths on your server.

Security & Notes
----------------
- The PKGBUILD depends on `ffmpeg` at runtime for video poster extraction; ensure `ffmpeg` is installed.
- Consider setting up a reverse proxy (nginx) in front of this service for TLS and host-based routing.
