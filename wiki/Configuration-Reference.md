[Home](Home.md) · [فارسی](fa-Configuration-Reference.md)

# Configuration Reference

Everything the interface can set is stored in one JSON file, so it can be read, diffed, version
controlled and edited by hand. This page documents it exhaustively.

## Where it lives

| Platform | Path |
| --- | --- |
| macOS | `~/Library/Application Support/net.nanternet.monitor/targets.json` |
| Linux | `~/.config/net.nanternet.monitor/targets.json` |
| Windows | `%APPDATA%\net.nanternet.monitor\targets.json` |

Writes are atomic: the new content goes to a `.staging` sibling and is then renamed over the original.
An interrupted save therefore cannot leave you with a truncated file and no targets.

The app reads the file once at startup. Edit it by hand while the app is running and your changes will
be overwritten by the next save from the interface. Quit first.

## Top level

```json
{
  "targets": [],
  "settings": {}
}
```

Both keys are optional; a missing or absent file yields no targets and default settings.

## Target

```json
{
  "id": "9f2c1e64-6b9a-4c0e-9f1a-8e5d3c7b2a10",
  "name": "Company site",
  "enabled": true,
  "intervalSecs": 120,
  "alerts": null,
  "spec": {}
}
```

| Field | Type | Default | Notes |
| --- | --- | --- | --- |
| `id` | string | generated | A UUID. Leave it empty when adding a target by hand and one is assigned on first save. Changing it orphans the stored secret. |
| `name` | string | required | What appears in the list and in alerts. Trimmed. |
| `enabled` | boolean | `true` | `false` pauses checks without losing configuration. |
| `intervalSecs` | number | `120` | Seconds between checks, floored at 15. |
| `alerts` | `"all"`, `"problems"`, `"critical"`, `"off"` or null | `null` | When this target is worth a notification. `null` follows the global `alerts` setting. |
| `spec` | object | required | Discriminated by `kind`. |

## Website spec

```json
{
  "kind": "website",
  "url": "https://example.ir",
  "externalProbe": true,
  "externalIntervalSecs": 900,
  "dnsProbe": false,
  "expectedStatus": null,
  "expectedBody": null,
  "iranNodes": 3,
  "abroadNodes": 4,
  "iranAlertThreshold": 1,
  "abroadAlertThreshold": 1
}
```

| Field | Type | Default | Notes |
| --- | --- | --- | --- |
| `url` | string | required | `https://` is prepended when no scheme is present. |
| `externalProbe` | boolean | `true` | Off turns this into a single-vantage-point monitor and the Iran-access verdict becomes unreachable. |
| `externalIntervalSecs` | number | `900` | Floored at 300 and never below `intervalSecs`. Governs the DNS check too. |
| `dnsProbe` | boolean | `false` | Resolve through every configured DNS resolver and fetch the site through each answer. Costs one request per resolver, which is why it is off by default. |
| `expectedStatus` | number or null | `null` | `null` accepts anything below 400. |
| `expectedBody` | string or null | `null` | Case-insensitive substring search of the response body. |
| `iranNodes` | number | `3` | Iranian vantage points, capped at 8. `0` disables them. |
| `abroadNodes` | number | `4` | Foreign vantage points, capped at 8. |
| `iranAlertThreshold` | number | `1` | How many Iranian nodes must fail before `website.ispPartial` is raised. `0` silences it. Clamped to `iranNodes`. |
| `abroadAlertThreshold` | number | `1` | How many foreign nodes must fail before `website.abroadPartial` is raised. `0` silences it. Clamped to `abroadNodes`. |

Node counts are upper bounds. Inside Iran one node is picked per ASN. Abroad the countries are dealt
out across continents in the order Asia, Europe, North America, South America, Oceania, Africa, so
four foreign nodes buy four continents rather than four neighbours. Asking for more than there are
distinct networks simply gets you all of them.

Both thresholds gate only the partial-failure warnings. Losing every node on one side of the border,
or everywhere at once, is always reported regardless.

## Server spec

```json
{
  "kind": "server",
  "host": "203.0.113.10",
  "port": 22,
  "username": "root",
  "auth": { "method": "key", "path": "~/.ssh/id_ed25519" },
  "hostFingerprint": "SHA256:hpQ0k9...",
  "loadLimit": 1.5,
  "memoryLimit": 85.0,
  "diskLimit": 85.0,
  "mounts": ["/", "/mnt/backup"],
  "containers": ["nginx", "postgres"]
}
```

| Field | Type | Default | Notes |
| --- | --- | --- | --- |
| `host` | string | required | Hostname or IP. |
| `port` | number | `22` | |
| `username` | string | required | |
| `auth` | object | required | See below. |
| `hostFingerprint` | string or null | `null` | SHA256 fingerprint, written automatically on first connection. Set to `null` to re-learn. |
| `loadLimit` | number | `1.5` | One-minute load average per core. Critical above three times this. |
| `memoryLimit` | number | `85.0` | Percentage. Critical at 97 regardless. |
| `diskLimit` | number | `85.0` | Percentage. Critical at 97 regardless. |
| `mounts` | string array | `["/"]` | Mount points exactly as `df` prints them. Empty resets to `["/"]`. |
| `containers` | string array | `[]` | Empty skips the Docker command entirely. |

### auth

```json
{ "method": "key", "path": "~/.ssh/id_ed25519" }
{ "method": "password" }
```

`~/` is expanded against `HOME`. Neither the password nor the key passphrase appears here; both live
in the keychain.

## Domain spec

```json
{
  "kind": "domain",
  "domain": "example.ir",
  "port": 443,
  "checkCertificate": true,
  "checkRegistration": true,
  "warnDays": 14
}
```

| Field | Type | Default | Notes |
| --- | --- | --- | --- |
| `domain` | string | required | Lower-cased; any scheme and trailing slash are stripped. |
| `port` | number | `443` | For the TLS handshake. |
| `checkCertificate` | boolean | `true` | |
| `checkRegistration` | boolean | `true` | Has no effect for `.ir`, where no registry publishes the date. |
| `warnDays` | number | `14` | Clamped to 1 through 120. Critical within a third of this. |

## Settings

```json
{
  "language": "en",
  "desktopNotifications": true,
  "alerts": "all",
  "discordWebhook": null,
  "telegramChatIds": [],
  "proxyUrl": null,
  "dnsResolvers": [],
  "confirmations": 2,
  "startAtLogin": false
}
```

| Field | Type | Default | Notes |
| --- | --- | --- | --- |
| `language` | `"en"` or `"fa"` | `"en"` | Anything else falls back to `"en"`. Applies to the interface and to alert text. |
| `desktopNotifications` | boolean | `true` | The OS notification permission still has the final say. |
| `discordWebhook` | string or null | `null` | Blank strings are normalised to `null`. |
| `alerts` | `"all"`, `"problems"`, `"critical"` or `"off"` | `"all"` | Which severities are worth a notification, on every channel. A target's own `alerts` overrides it. |
| `telegramChatIds` | string array | `[]` | Every alert goes to all of them. Numeric IDs or public `@name`s, validated on save. Duplicates and blanks are dropped, capped at 32. A single legacy `telegramChatId` string is folded in as the first entry on load. |
| `proxyUrl` | string or null | `null` | Proxy for Discord and Telegram delivery only. Scheme must be `http`, `https`, `socks5` or `socks5h`; anything else is rejected on save. Site checks ignore it. |
| `dnsResolvers` | string array | `[]` | Extra resolver IP addresses, added to the built-in list. IPs only, hostnames are rejected on save. Capped at 12 including built-ins. |
| `confirmations` | number | `2` | Consecutive failing checks before alerting. Clamped to 1 through 10. |
| `startAtLogin` | boolean | `false` | Toggling this registers or removes a real login item. |

## Secrets

Secrets are never written to `targets.json`. They go to the platform credential store, under service
name `IranNANternet`:

| Account | Holds |
| --- | --- |
| `target.<target id>` | SSH password or private key passphrase |
| `notifications.telegram-token` | Telegram bot token |

- macOS: Keychain, inspectable with Keychain Access
- Windows: Credential Manager
- Linux: Secret Service, via GNOME Keyring or KWallet

Deleting a target deletes its secret. Copying `targets.json` to another machine carries the targets
but not the secrets, which is the intended behaviour; re-enter them there.

On a headless Linux box with no Secret Service provider, secret storage fails and password-authenticated
servers report `server.authenticationFailed`. Key-based targets with unencrypted keys are unaffected,
since they need no secret at all.

## Timings that are not configurable

Deliberately fixed, because exposing them would be exposing a way to break the app:

| Value | Where |
| --- | --- |
| minimum check interval, 15 s | any target |
| minimum external probe interval, 300 s | website, to respect check-host rate limits |
| registration lookup interval, 12 h | domain, to respect registry rate limits |
| check-host node list cache, 6 h | website |
| check-host result polling budget, 28 s | website |
| SSH connect timeout, 12 s | server |
| SSH command timeout, 25 s | server |
| TLS connect timeout, 20 s | domain |
| TLS handshake timeout, 15 s | domain |
| local HTTP request timeout, 20 s | website |
| DNS query timeout, 4 s | website, per resolver |
| DNS pinned fetch timeout, 15 s | website, per resolver |
| resolver count ceiling, 12 | website, built-ins plus your own |
| critical usage escalation, 97 % | server memory and disk |
| scheduler tick, 1 s | all |

The scheduler wakes once a second, runs every target whose next due time has passed, and dispatches
each check as an independent task. One slow server therefore cannot delay anything else.

## Hand editing safely

1. Quit the app from the tray menu. Not just the window.
2. Back up the file.
3. Edit, keeping it valid JSON. An unparseable file makes the app start with no targets rather than
   crash, but it will not be rescued for you.
4. Start the app and confirm your targets appear.

Adding a target by hand needs only `name` and `spec`; `id`, `enabled` and `intervalSecs` fill in from
defaults, and the id is generated the first time the app saves.
