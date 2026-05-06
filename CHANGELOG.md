# Changelog
All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]
### ℹ Changed
- Switched from `slog` to `tracing` for logging
- Update dependencies (updates some exposed types)

## [0.2.0] - 2021-05-12
### ✨ Added
- 🎵 An audio queue that automatically sort, decode and handle lost audio packets
- Audio example for `tsclientlib`
- Send audio packets with `Connection::send_audio`
- ⏫ Automatically improve identity level if it is not high enough
- 🔓 Add `channel`, `channel_password` and (server-)`password` to `ConnectOptions`
- 📂 Filetransfer: Download and upload files from and to TeamSpeak servers
- Most structs can be serialized with `serde` now
- `Reason` to events that support it, that allows distinguishing between client joins and subscription events
- Add ability to check server identity when connecting as `ConnectOptions::server`
- `StreamItem::MessageEvent` for non-book events

### ℹ Changed
- ➠ Upgrade from `futures` 0.1 to 0.3 and `tokio` 0.1 to 1.0 for `async`/`await` support
- 🔒 Rewrite `tsproto` and `tsclientlib` to be a single, non-cloneable object. This removes all locks
- 🚀 Many performance improvements
	- New command parser, 2–2.5× faster than the old one
	- New command serializer, 2× faster than the old one
- Switched error handling library from `failure` to `thiserror`
- Now licensed under MIT/Apache 2.0 (previously under OSL 2.0)
- Commands are now sent with `command.send(&mut connection)?`
- A new connection should now be created with `ConnectOptions::connect` instead of `Connection::new`
- Multiple renamings in the book
- Rename `StreamItem::ConEvents` to `StreamItem::BookEvents`
- Rename `UidRef` to `Uid` and make it a dynamically sized type. The previously owned `Uid` type is now called `UidBuf`
- Replace `ring` with RustCrypto crates to get rid of the `flakebi-ring` crate

### ❌ Removed
- The `ConnectionManager` in `tsproto` was removed

### 🐛 Fixed
- Hashcash implementation counts leading zeroes from lsb instead of msb
- Use `3.?.?` version by default to allow connecting to newer TeamSpeak servers
- Fix channel order handling
- Use `git-testament` instead of `built`, this removes building libgit
- Fix encoding newlines in commands
- The last `Ack` packet is sent reliably now, previously it was lost sometimes
- Fix connecting to servers behind certain protections by restart sending init packets
- Try to reconnect infinitely after a timeout
- Keep channel and muted state when reconnecting
- Automatically subscribe to new channels when the server is subscribed

## [0.1.0] - 2019-04-14
### Added
- First release on crates.io for
	- tsclientlib
	- tsproto
	- tsproto-commands
	- tsproto-structs
