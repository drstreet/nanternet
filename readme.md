# IranNANternet

**English** · [فارسی](readme.fa.md) · [Wiki](wiki/Home.md)

A desktop monitor that tells a real outage apart from a regional block.

If you run anything behind an Iranian network you already know the failure mode: the site loads
perfectly from your desk, so nothing looks wrong, while every visitor outside the country sees a
timeout. Or the reverse: it is up everywhere except on one mobile operator. Classic uptime monitors
answer "is it up?" from a single vantage point, which is the one question that cannot distinguish
those cases.

IranNANternet probes each site from your own network **and** from vantage points inside and outside
Iran at the same time, then names the failure instead of just flagging it. It also watches SSH hosts,
Docker containers, TLS certificates and domain registrations, so one window covers the things that
usually break quietly.

Built with Tauri, Rust and React. Runs on macOS, Windows and Linux.

## What it watches

### Websites, with a verdict

Every check combines a direct request from your machine with an HTTP probe run through
[check-host.net](https://check-host.net) nodes. One node is picked per network, so a single ISP
dropping the route stands out instead of averaging away. The two results are then compared:

| Your network | Outside Iran | Inside Iran | Verdict |
| --- | --- | --- | --- |
| reachable | all fail | — | **Iran-access isolation** — only reachable from inside the country |
| fails | some succeed | — | **Blocked on this network** — your ISP or DNS, not the server |
| — | some succeed | all fail | **Unreachable from inside Iran** — filtered inbound |
| fails | all fail | all fail | **Down** — an actual outage |
| reachable | reachable | some fail | **Partial ISP outage** — warning with the failing networks named |

An HTTP error still proves the host answered, so a 404 counts as reachable and never gets reported
as an outage. Nodes that have not finished are treated as unknown rather than as failures.

Because check-host.net rate limits requests, the external probe runs on its own slower schedule
(15 minutes by default) while the local request keeps its short interval. The most recent external
result is carried forward between probes.

### Websites, through several DNS resolvers

Optional per target. The address is resolved by each resolver in turn, and then the site is fetched
**through every answer**, which is what separates a poisoned name from a blocked address:

| Condition | Verdict |
| --- | --- |
| a resolver returns a non-routable address while others return a real one | **DNS is answering with a fake address** — the shape of DNS-level filtering |
| your network cannot reach the site but some resolver's address can | **Another resolver still reaches it** — switching resolver restores access |
| no resolver answers at all | **No resolver answered** — outbound port 53 is likely blocked, the site is not implicated |

Shecan, 403.online, Begzar, Electro, Cloudflare and Google are built in; add any resolver by IP in
Settings. DNS is spoken directly over UDP/53 rather than through a resolver library, because the
resolvers that matter here offer no DNS-over-HTTPS.

Resolvers that merely disagree on the address are not reported. A CDN hands a different edge to every
resolver, and treating that as a fault would be a false-alarm machine.

### Servers over SSH

One connection per check runs a single batched command and parses the result locally:

- load average normalised per core, read from `/proc/loadavg` and `nproc`
- memory from `/proc/meminfo`, preferring `MemAvailable` and falling back to free + buffers + cached
- disk usage per mount point from `df -Pk`, matching the way `df` reports capacity
- container state and healthcheck from `docker ps`

Nothing is installed on the server and no agent is needed. Thresholds are per target, and usage
above 97% escalates from a warning to critical because that is where backups start failing.

Authentication accepts a private key or a password. Keys are recommended and the interface says so.
The host key is pinned on first use: the fingerprint is recorded, and a later change is reported as
critical instead of being silently accepted.

Container names are matched exactly first, then by substring, so `db` also matches a Compose
container called `stack-db-1`.

### Certificates and domains

TLS expiry is read from the certificate presented during a real handshake, so what you see is what
visitors get. Severity climbs as the deadline approaches rather than firing one flat warning.

Registration expiry comes from RDAP, looked up twice a day rather than every cycle, because the date
changes once a year and registries throttle repeat queries.

`.ir` domains are a documented exception: IRNIC's whois server filters expiry dates out of its
answers and there is no `.ir` RDAP service, so the date is not obtainable by any means. Rather than
report a lookup failure you cannot act on, those domains are marked as unpublished and the
certificate check still runs normally.

The registrable domain is derived with a small suffix heuristic rather than a full public suffix
list, which is correct for common cases including `.co.uk` and `.co.ir` but is not exhaustive.

## Notifications

Alerts go to native desktop notifications, a Discord webhook, and a Telegram bot, in whichever
combination is configured. Two behaviours keep them useful:

- an alert fires only when the verdict actually changes, not on every cycle
- a failure must repeat a configurable number of times before it is announced, so one dropped packet
  does not wake you up. Recoveries are always announced immediately.

Telegram and Discord are both blocked in Iran, so alerts can be routed through a proxy: point the
setting at the local inbound your v2ray, Xray or Hiddify client already listens on, usually
`socks5h://127.0.0.1:10808`. Site checks deliberately ignore it, otherwise they would be measuring
the tunnel instead of your network.

Closing the window leaves the app running in the tray so checks continue.

## Where your data lives

Targets and settings are stored as plain JSON in the platform config directory, written atomically so
an interrupted save cannot truncate the file:

- macOS `~/Library/Application Support/net.nanternet.monitor/targets.json`
- Linux `~/.config/net.nanternet.monitor/targets.json`
- Windows `%APPDATA%\net.nanternet.monitor\targets.json`

SSH passwords, key passphrases and the Telegram bot token never touch that file. They go to the
operating system credential store: Keychain on macOS, Credential Manager on Windows, Secret Service
on Linux.

## Running it

Requires Node 20+ and a Rust toolchain, plus the
[Tauri system prerequisites](https://v2.tauri.app/start/prerequisites/) for your platform.

```sh
npm install
npm run app          # development window with hot reload
npm run app:build    # release bundle for the current platform
```

Backend tests cover the parsers and the verdict logic, which are the parts that fail silently:

```sh
cd src-tauri && cargo test
```

`tools/make-icon.py` regenerates the app icon from scratch with no image dependencies; feed its
output to `npx tauri icon`.

## Layout

```
src/                    React interface
  components/           Dashboard, ServerList, AddServerModal, Settings
  i18n/                 English and Persian dictionaries, RTL handling
  useMonitor.ts         command and event bridge to the backend
src-tauri/src/
  network_checker.rs    local probe and the verdict rules
  checkhost.rs          check-host.net client, node selection, result parsing
  dns_probe.rs          UDP/53 resolution per resolver, then a pinned fetch
  ssh_manager.rs        SSH transport, host key pinning, resource parsing
  docker_monitor.rs     container state and healthcheck parsing
  cert_watch.rs         TLS handshake inspection, RDAP and whois
  notification_engine.rs  desktop, Discord and Telegram delivery
  scheduler.rs          due-time loop, alert transitions
  secrets.rs            OS credential store
```

## Language

The interface ships in English and Persian, with full right-to-left layout. Adding a language means
adding one dictionary file next to `src/i18n/en.ts` and one entry in `src/i18n/index.tsx`.

## Documentation

The `wiki/` directory holds the full manual in both languages, starting at
[wiki/Home.md](wiki/Home.md). It covers every setting, the exact shape of the config file, and what
to do when a check reports something you did not expect.

Those files are laid out as GitHub wiki pages, so publishing them is a push:

```sh
git clone https://github.com/drstreet/nanternet.wiki.git /tmp/nanternet-wiki
cp wiki/*.md /tmp/nanternet-wiki/
cd /tmp/nanternet-wiki && git add . && git commit -m "Sync wiki" && git push
```

## Licence

MIT. See [LICENSE](LICENSE).
