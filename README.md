# TsProto

This repository implements the TeamSpeak3 protocol for usage in clients and bots.

If you are searching for a usable client, [Qint](https://github.com/ReSpeak/Qint) is a cross-platform TeamSpeak client, which ~~is~~ will be using this library.

## Projects

 - tsproto: The low level library that does the network part
 - tsproto-bookkeeping: Store information when connected to a server

## Clone

```
git clone https://github.com/ReSpeak/tsclientlib.git
cd tsclientlib
git submodule update --init --recursive
```

## Build and run the example client

You need to install rust (preferred installation method is [rustup](https://rustup.rs/)) for building these projects.

```
cd tsproto
cargo run --example client
```

## Miscellaneous

This project is not an official TeamSpeak project. We started to write an own client for fun and because we want some features (and bugfixes) which are not available in the official client.

That said, we do not want to harm the company behind TeamSpeak because we like their product. Otherwise we would just use something else and not write our own client 😉. As TeamSpeak earns its money with selling servers and thus their existence depends on it, we will not publish any server related code and we encourage you to do the same.

Thanks TeamSpeak for your software!

License
-------
Licensed under either of

 * [Apache License, Version 2.0](LICENSE-APACHE)
 * [MIT license](LICENSE-MIT)

at your option.

### Contribution

Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in the work by you, as defined in the Apache-2.0 license, shall be dual licensed as above, without any additional terms or conditions.
