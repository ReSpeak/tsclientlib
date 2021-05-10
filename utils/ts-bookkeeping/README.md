# ts-bookkeeping [![docs.rs](https://docs.rs/ts-bookkeeping/badge.svg)](https://docs.rs/ts-bookkeeping)
`ts-bookkeeping` contains structs to store the state of a TeamSpeak server, with its clients and
channels.

The crate can be used to keep track of the state on a server by processing all incoming
commands, which is why it is called “bookkeeping”. It also contains generated structs for all
TeamSpeak commands.

Incoming commands can be applied to the state and generate [`events`]. The main struct is
[`data::Connection`].

The structs have methods to create packets for various actions. The generated packets can be
sent to a server.

## License
Licensed under either of

 * [Apache License, Version 2.0](../../LICENSE-APACHE)
 * [MIT license](../../LICENSE-MIT)

at your option.
