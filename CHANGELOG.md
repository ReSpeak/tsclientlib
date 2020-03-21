# Changelog
All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]
### Added
- Automatically improve identity level if it is not high enough
- Add `channel`, `channel_password` and (server-)`password` to `ConnectOptions`
- Filetransfer: Download and upload files from and to TeamSpeak servers
- Audio example for `tsclientlib`
- Most structs can be serialized with `serde` now

### Changed
- Upgrade from `futures` 0.1 to 0.3 and `tokio` 0.1 to 0.2 for `async`/`await` support
- Rewrite `tsproto` and `tsclientlib` to be a single, non-clonable object. This removes all locks.
- Many performance improvements
	- New command parser, 2×–2.5× faster than the old one
	- New command serializer, 2× faster than the old one
- Switched error handling library

### Fixed
- Hashcash implementation counts leading zeroes from lsb instead of msb
- Use `3.?.?` version by default to allow connecting to newer TeamSpeak servers
- Fix channel order handling
- Use `git-testament` instead of `built`, this removes building libgit
- Fix encoding newlines in commands
- The last `Ack` packet is sent reliably now, previously it was sometimes lost

## [0.1.0] - 2019-04-14
### Added
- First release on crates.io for
	- tsclientlib
	- tsproto
	- tsproto-commands
	- tsproto-structs
