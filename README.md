# TsClientlib
The various libraries in this repository are currently a work in progress so don‘t expect things to work. At the moment, connecting and basic audio is working.

This repository implements the TeamSpeak3 protocol for usage in clients and bots.

If you are searching for a usable client instead of a library, [Qint](https://github.com/ReSpeak/Qint) is a cross-platform TeamSpeak client, which is based on this library.

## Dependencies
- [OpenSSL](https://www.openssl.org) 1.1
- [GStreamer](https://gstreamer.freedesktop.org)
- [Rust](https://rust-lang.org) (preferred installation method is [rustup](https://rustup.rs))

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
cargo run --features audio --example audio
```


## Projects
You probably want to use tsclientlib, it builds upon all other projects. This list shows what each library does and it may be useful if you want to help developing.

- gst-plugin-ts3: A gstreamer demuxer that takes TeamSpeak audio data and creates a new pad for every client and codec.
- tsclientlib: This is the main product of this repository, a simple to use TeamSpeak library
- tsclientlib-ffi: A thin wrapper around tsclientlib, which exports C functions so the library can be used in Qint.
- tsproto: The low level library that does the network part. You probably don't want to use that, but a higher level library like tsclientlib.
- tsproto-audio: Creates gstreamer pipelines and generally manages audio stuff to be easy to use.
- tsproto-commands: Parse commands into structs (messages) and contains basic types and enums for TeamSpeak.
- tsproto-util: Contains some utility for code generation from [tsdeclarations](https://github.com/ReSpeak/tsdeclarations).

## Miscellaneous
This project is not an official TeamSpeak project. We started to write an own client for fun and because we want some features (and bugfixes) which are not available in the official client.

That said, we do not want to harm the company behind TeamSpeak because we like their product. Otherwise we would just use something else and not write our own client 😉. As TeamSpeak earns its money with selling servers and thus their existence depends on it, we will not publish any server related code and we encourage you to do the same (this may change with the new licensing system, introduced in the 3.1 server).

Thanks TeamSpeak for your software!

## License
Licensed under either of

 * [Apache License, Version 2.0](LICENSE-APACHE)
 * [MIT license](LICENSE-MIT)

at your option.
