# tsproto-commands &emsp; [![docs.rs](https://docs.rs/tsproto-commands/badge.svg)](https://docs.rs/tsproto-commands)
`tsproto-commands` lets you deserialize structs from TeamSpeak commands and
serialize commands from structs. The crate also contains enums for all
errors used in the TeamSpeak protocol and a list of all known versions of
the TeamSpeak client, including their signature.

The underlying data files can be found in the [tsdeclarations](https://github.com/ReSpeak/tsdeclarations)
repository.

The contained data may change with any version so the suggested way of
referring to this crate is using `tsproto-commands = "=0.1.0"`.

## License
Licensed under either of

 * [Apache License, Version 2.0](LICENSE-APACHE)
 * [MIT license](LICENSE-MIT)

at your option.
