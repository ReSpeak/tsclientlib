# TsClientlib &emsp; [![docs.rs](https://docs.rs/tsclientlib/badge.svg)](https://docs.rs/tsclientlib)
tsclientlib is a library which makes it simple to create TeamSpeak clients
and bots.

If you want a full client application, you might want to have a look at
[Qint].

If you need more power over the internals of a connection, you can enable the `unstable`
feature. Beware that this functionality may change on any minor release.

The base class of this library is the [`Connection`]. One instance of this
struct manages a single connection to a server.

[`Connection`]: struct.Connection.html
[Qint]: https://github.com/ReSpeak/Qint

## License
Licensed under either of

 * [Apache License, Version 2.0](../LICENSE-APACHE)
 * [MIT license](../LICENSE-MIT)

at your option.
