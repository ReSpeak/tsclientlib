# TsClientlib [![Build status](https://ci.appveyor.com/api/projects/status/ylne3ku7265xa22j/branch/master?svg=true)](https://ci.appveyor.com/project/Flakebi/tsclientlib/branch/master)
TsClientlib is a library that enables you to write voip clients and bots using
the TeamSpeak protocol.

> TeamSpeak is a proprietary voice-over-Internet Protocol (VoIP) application for
> audio communication between users on a chat channel, much like a telephone
> conference call. Users typically use headphones with a microphone. The client
> software connects to a TeamSpeak server of the user's choice, from which the
> user may join chat channels. – [Wikipedia](https://en.wikipedia.org/wiki/Teamspeak)

The various libraries in this repository are currently a work in progress so
don‘t expect everything to work. Many things you would expect from a
library are working, like connecting, getting events, writing and receiving
messages, etc. Some features like automatic reconnects are not yet there.

If you are searching for a usable client instead of a library,
[Qint](https://github.com/ReSpeak/Qint) will be a cross-platform TeamSpeak
client, which is based on this library (but it is not ready yet).

## Dependencies
- [Rust](https://rust-lang.org) (preferred installation method is [rustup](https://rustup.rs))
- [OpenSSL](https://www.openssl.org) 1.1 (linux only)

## Getting Started
An example of a simple chat bot can be found [here](https://github.com/ReSpeak/SimpleBot).

## Clone
This repository embeds the [declarations](https://github.com/ReSpeak/tsdeclarations) as submodule. You can clone with
```
git clone https://github.com/ReSpeak/tsclientlib.git --recurse-submodules
```
or download the submodule afterwards with
```
git clone https://github.com/ReSpeak/tsclientlib.git
cd tsclientlib
git submodule update --init --recursive
```

## Build and run the example client
The code can be found [here](tsclientlib/examples/simple.rs).
```
cd tsclientlib
cargo run --example simple
```

## Build and run the example audio client
The code can be found [here](tsclientlib/examples/audio.rs).
```
cd tsclientlib
cargo run --example audio
```


## Projects
You probably want to use tsclientlib, it builds upon all other projects. This
list shows what each library does and it may be useful if you want to help
developing.

- `tsclientlib`: This is the main product of this repository, a simple to use TeamSpeak library
- `tsproto`: The low level library that does the network part. You probably don't want to use that, but a higher level library like tsclientlib.

### Utils
The utils folder contains smaller building blocks for the library.

- `ts-bookkeeping`: This crates keeps book of the currently connected clients and channels of a server.
- `tsproto-packets`: Parse packets and commands.
- `tsproto-structs`: Contains parsed versions of the [tsdeclarations](https://github.com/ReSpeak/tsdeclarations).
- `tsproto-types`: Contains basic types for TeamSpeak, e.g. versions and error codes.

## How this works
`tsproto` implements the basic TeamSpeak protocol stuff (uh, who would have
expected that), which comes down to creating a connection, make sure that udp
packets are delivered, encrypt and compress the communication and give access to
all these low-level things.

The convenient client library on top is `tsclientlib`. It uses all the versions,
messages, structures and errors which are written down in a
[machine readable format](https://github.com/ReSpeak/tsdeclarations) and
provides a nice and safe api.

## Performance
For benchmarks, a TeamSpeak server has to run at localhost. Run
```
cd tsproto
cargo bench -- <name>
```
where `<name>` is `connect` or `message`.

On a i7-5280K with 6 cores/12 threads @3.6 GHz, which uses more or less only one
thread because we only test a single connection, we get:

- 199 ms for creating one connection, which means 6.5 connections per second. This comes down to solving the RSA puzzle at the beginning of a connection, not much else plays a role. Be sure to run the benchmark with `--features rug` to get an efficient big integer implementation, otherwise it will fall back to a slower implementation.
- 189 µs for sending a message, which means 5300 messages per second. The benchmark is not perfectly accurate as it does not wait for all packets to be sent.

## Miscellaneous
This project is not an official TeamSpeak project. We started to write an own
client for fun and because we want some features (and bugfixes) which are not
available in the official client.

That said, we do not want to harm the company behind TeamSpeak because we like
their product. Otherwise we would just use something else and not write our own
client 😉. As TeamSpeak earns its money with selling servers and thus their
existence depends on it, we will not publish any server related code and we
encourage you to do the same (this may change with the new licensing system,
introduced in the 3.1 server).

Thanks TeamSpeak for your software!

## License
Licensed under either of

 * [Apache License, Version 2.0](LICENSE-APACHE)
 * [MIT license](LICENSE-MIT)

at your option.
