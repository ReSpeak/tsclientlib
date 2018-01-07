# TsClientlib

This repository implements the TeamSpeak3 protocol for usage in clients and bots.

If you are searching for a usable client, [Qint](https://github.com/ReSpeak/Qint) is a cross-platform TeamSpeak client, which ~~is~~ will be using this library.

## Projects

 - tsproto: The low level library that does the network part. You probably don't want to use that, but a higher level library like tsclientlib.
 - tsproto-commands: Parse commands into structs
 - tsclientlib: This is the main product of this repository, a simple to use TeamSpeak library

## Clone

```
git clone --recurse-submodules https://github.com/ReSpeak/tsclientlib.git
```

## Build and run the example client

You need to install rust (preferred installation method is [rustup](https://rustup.rs/)) to build these projects.

```
cd tsclientlib
cargo run --example simple
```

## Miscellaneous

This project is not an official TeamSpeak project. We started to write an own client for fun and because we want some features (and bugfixes) which are not available in the official client.

That said, we do not want to harm the company behind TeamSpeak because we like their product. Otherwise we would just use something else and not write our own client 😉. As TeamSpeak earns its money with selling servers and thus their existence depends on it, we will not publish any server related code and we encourage you to do the same.

Thanks TeamSpeak for your software!

## License

Licensed under either of

 * [Apache License, Version 2.0](LICENSE-APACHE)
 * [MIT license](LICENSE-MIT)

at your option.
