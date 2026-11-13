# composeports

A small Rust CLI and library that lists every host port published by one or more Docker Compose files and reports conflicts before `docker compose up` fails with "port is already allocated".

## Why

Running several apps on one server usually means several Compose files written at different times. Two of them publishing `8080`, or a database bound to all interfaces in one project and to `127.0.0.1` in another, only shows up when a deploy fails. composeports reads all the files at once and tells you.

## Install

```
cargo install composeports
```

## Usage

```
composeports ~/apps/*/compose.yml
```

Example output:

```
 5432/tcp 127.0.0.1       -> 5432  db (/srv/shop/compose.yml)
 8080/tcp *               -> 80    web (/srv/blog/compose.yml)
 8080/tcp *               -> 3000  app (/srv/shop/compose.yml)
CONFLICT 8080/tcp: web (/srv/blog/compose.yml) and app (/srv/shop/compose.yml)
```

The exit code is 0 with no conflicts, 1 when bindings clash and 2 for usage or parse errors, so it works as a pre-deploy check in CI.

It understands the short syntax (`"8080:80"`, `"127.0.0.1:5432:5432"`, `"53:53/udp"`, `"[::1]:6379:6379"`, port ranges) and the long syntax with `target`, `published`, `host_ip` and `protocol`. Entries that only name a container port publish a random host port and are skipped. Two bindings conflict when they share a port and protocol and either one listens on all interfaces or both use the same IP. Variable interpolation such as `${PORT}` is not expanded; run `docker compose config` first if you rely on it.

As a library:

```rust
let bindings = composeports::parse(&std::fs::read_to_string("compose.yml")?, "compose.yml")?;
for (a, b) in composeports::conflicts(&bindings) {
    eprintln!("{} used by {} and {}", a.host_port, a.service, b.service);
}
```

## Fewer published ports

Most web apps do not need a host port at all. Put a reverse proxy on 80 and 443, attach apps to a shared Docker network and route by hostname. The guide on [how to run multiple apps on one VPS](https://peon.sh/blogs/how-to-run-multiple-apps-on-one-vps) walks through that layout. Built by a contributor to [Peon](https://peon.sh/), an open-source self-hosted deployment platform.

MIT licensed.
