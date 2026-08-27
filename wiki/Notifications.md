[Home](Home.md) · [فارسی](fa-Notifications.md)

# Notifications

An alerting system's real problem is not delivery, it is credibility. A monitor that cries wolf gets
muted, and a muted monitor is worse than none because you believe you are covered. Two rules exist to
keep that from happening, and they matter more than the channels do.

## When an alert fires

**Only on a change of verdict.** A target that has been critical for six hours does not re-notify
every two minutes. The current level and the exact set of finding codes are remembered; an alert goes
out only when one of them differs from what was last announced.

The finding codes are part of the comparison on purpose. A server that goes from `server.diskHigh` to
`server.diskHigh` plus `docker.notRunning` is still critical, but it is now critical for a new reason,
and that is worth telling you.

**Only after the failure repeats.** `Failed checks before alerting` in Settings, default 2, is how
many consecutive checks must agree on a non-healthy level before it is announced. One dropped packet
on a bad link produces one failed check and no notification.

Recoveries bypass the counter entirely: the moment a target reads healthy, the recovery is announced.
Waiting to tell you something is fixed would be a strange kind of caution.

The counter is per level, not per target. Critical, then warning, then critical restarts it, because
a target flapping between levels has not confirmed anything.

There is also a first-run rule: the app does not announce a healthy verdict for a target it has never
seen before. Otherwise every launch would open with a burst of "recovered" notifications for things
that were never broken.

### Choosing the confirmation count

| Value | Behaviour |
| --- | --- |
| 1 | alert on the first failed check, fastest and noisiest |
| 2 | the default, filters single transient failures |
| 3 or more | for links so unreliable that two failures in a row still prove nothing |

Time to alert is roughly the check interval multiplied by this number. With a 2 minute interval and
the default of 2, expect to hear within about 4 minutes.

### Choosing which verdicts are worth hearing

`Notify me about` in Settings decides which severities actually leave the app, on every channel at
once.

| Value | Behaviour |
| --- | --- |
| Everything, including recoveries | the default, every change of verdict |
| Warnings and worse | nothing while a target is healthy |
| Critical only | warnings stay in the app, only outages reach you |
| Nothing | the app keeps watching and shows everything, but sends nothing |

Recoveries are themselves a change of verdict to healthy, so anything other than the first option means
no "back to normal" message either. That is the trade: a channel that only speaks when something is
wrong cannot also tell you when it stops being wrong.

Every target has the same setting on its own edit screen, plus a fifth option to follow the global one,
which is the default. This is how a staging site that goes down nightly stays quiet while production
still shouts. The confirmation counter and the change-of-verdict rule run first and are unaffected: a
suppressed alert is one the app decided was worth sending and you decided not to receive, so silencing
a target does not leave stale state behind when you turn it back on.

## Channels

All configured channels get every alert. There is no per-channel severity filter, because a system
where the important alerts go somewhere you do not read is a system that has already failed.

### Desktop notifications

On by default, using the operating system's own notification centre. macOS will ask for permission the
first time; if you decline, they stop appearing and nothing in the app can override that. Re-enable
them in System Settings under Notifications.

### Discord

Paste a webhook URL. In Discord: Server Settings, Integrations, Webhooks, New Webhook, then Copy
Webhook URL. It looks like `https://discord.com/api/webhooks/<id>/<token>`.

The message carries the same content as the Telegram one described below, with the grid wrapped in a
Markdown code fence so the columns stay aligned. Discord has no native table, so this is a monospace
block rather than a real one. Content is capped at 1900 characters, under Discord's limit, with an
ellipsis if it would run over.

Desktop notifications get the findings only. There is no monospace font and about two lines of room
there, so a grid would only be mangled and then cut off.

Treat the URL as a secret. Anyone holding it can post to that channel. It lives in the config file
rather than the keychain, because unlike a password it is a location as much as a credential, and
splitting it would make the config file unable to describe where alerts go.

### Telegram

Two values are needed.

The **bot token** comes from [@BotFather](https://t.me/botfather): send `/newbot`, follow the prompts,
and copy the token, which looks like `123456789:AAH...`. This is a real credential and goes to the
system keychain, not to the config file.

The **chats and channels** box is where messages go, one entry per line and up to 32 of them. Every
alert is delivered to all of them at once, in parallel, so one unreachable chat does not hold up the
rest. For a private chat with yourself, message [@userinfobot](https://t.me/userinfobot) to get your
numeric ID. For a channel or group, add the bot as a member first, then use the numeric ID, which for
channels starts with `-100`. A public channel can also be given as `@name`. Entries are checked on
save, so a typo is reported rather than silently dropped.

### What a Telegram alert looks like

Alerts go out through `sendRichMessage`, the rich message method added in Bot API 10.1, which means the
grid is a real `<table>` that the Telegram client lays out itself:

| ORIGIN | VANTAGE POINT | PING | RESULT |
| --- | --- | --- | --- |
| 🟢 LOCAL | your network | 143 ms | 200 |
| 🟢 IR | Tehran (IR) | 291 ms | 200 |
| 🔵 IR | Shiraz (IR) | 548 ms | 200 |
| 🟡 EU | Vienna (AT) | 1047 ms | 200 |
| 🟠 NA | Vancouver (CA) | 2482 ms | 200 |
| 🔴 AS | Jakarta (ID) | - | Connection timed out |

The dot grades that row against its own threshold. For a vantage point that is round-trip time: green
under 500 ms, blue under 1000, yellow under 2000, orange above it, and red when nothing answered. An
HTTP 4xx or 5xx caps the row at orange, because the host did reply. A white dot means the probe is
still running.

Server rows are graded against the limits you configured rather than a fixed number, so 80% memory is
yellow when your limit is 95 and orange when it is 85. Certificate and registration rows are graded
against that domain's own warning window.

Above it sits the heading, the address being watched, and one paragraph per finding: the localized
condition in bold with the raw measurement underneath. Below it is a link straight to the check-host
report for that run.

Websites get one row per vantage point plus your own network and each DNS resolver. Servers get load,
memory, disks and containers against their limits. Domains get the certificate and registration dates.

The table is always English, headings included. Persian headings in a right-to-left table put a Persian
word next to a Latin city name in the same row, and the result reordered on screen badly enough to be
worth losing the translation over.

Persian alerts are sent with `is_rtl`, so the message and the table are laid out right to left. Each
Latin run inside them is wrapped in a bidirectional isolate. Without it the bidirectional algorithm
reorders Latin text that sits at the edge of a right-to-left paragraph, which is how
`1 of 3 nodes failed` used to reach the reader as `of 3 nodes failed 1`.

Rich messages allow 32768 characters and 500 blocks, far more than an alert needs. The message is
capped anyway at 8 findings, 16 table rows and 44 characters per cell, so one long upstream error
cannot push the table off the screen.

### When rich messages are not available

If `sendRichMessage` is refused for any reason, the alert is immediately retried through plain
`sendMessage` with `parse_mode: HTML`, where the grid becomes a monospace `<pre>` block with columns
padded by hand. It is less pretty and it carries the same information.

Everything interpolated into either format is escaped, because finding details are arbitrary upstream
strings and a stray `<` in one would make Telegram reject the whole message. Only a real `https://`
report URL is allowed to become a link. Since escaping can grow a string fivefold, a `sendMessage`
fallback that still will not fit under 3900 characters is sent unformatted rather than truncated:
cutting HTML mid-tag loses the entire message, and losing an alert to a formatting error is not a
trade worth making.

Telegram's API is not reachable from most Iranian networks without a proxy, which is what the setting
below is for.

### Proxy for alerts

Telegram and Discord are both blocked in Iran, so a monitor that can watch a site but never report on
it is not much use. **Proxy for alerts** in Settings routes both channels through a proxy.

Point it at the local inbound your existing client already listens on:

| Client | Inbound |
| --- | --- |
| v2rayN | `socks5h://127.0.0.1:10808` by default, HTTP on `10809` |
| anything else | whatever its own settings screen reports as the local SOCKS or HTTP inbound |

Do not guess the port. Every client shows it, usually labelled local, inbound or listening port, and
Hiddify in particular has changed its default across versions.

Accepted schemes are `http`, `https`, `socks5` and `socks5h`. Anything else is refused when you save,
rather than failing silently later. Prefer `socks5h` over `socks5`: the `h` sends the hostname to the
proxy for resolution, so `api.telegram.org` is resolved on the far side of the tunnel instead of by
the resolver you are working around.

A raw `vmess://` or `vless://` link cannot be pasted here. Speaking those protocols would mean
shipping a v2ray core inside the app. Your client already does that job and exposes a local port; this
connects to that port.

If the proxy is misconfigured, the alert is dropped and the reason is written to standard error. It is
deliberately **not** retried directly: the proxy exists because the direct path does not work, and
sending your bot token down the very route you are avoiding is not a helpful fallback.

`HTTPS_PROXY` in the environment is still honoured when this setting is empty, so an existing setup
keeps working. The local website probe ignores both, so it keeps measuring your real network rather
than your tunnel. The DNS check ignores both as well, for the same reason.

## What an alert looks like

```
🔴 Backup server — critical

Disk space is running out: /mnt/backup is 98.2% full, 1.4 GiB free
Container is not running: pg-dump is exited (Exited (1) 6 minutes ago)
```

The title carries a coloured badge, the target name and the severity. The body is one line per
finding: the condition in your chosen language, then the raw measured detail. Both halves matter, the
first tells you what to think and the second lets you check whether to agree.

Alert text follows the language set in Settings, so switching the interface to Persian switches the
notifications too.

## Testing it

**Send a test alert** in Settings dispatches a healthy-level alert to every configured channel at
once. It uses the same code path as a real alert, so if the test arrives, real alerts will.

If nothing arrives:

- desktop only: check the OS notification permission for IranNANternet
- Discord only: the webhook URL is wrong or the webhook was deleted
- Telegram only: the token or chat ID is wrong, or the bot was never added to the channel
- neither Discord nor Telegram, while desktop works: the network cannot reach either API, which is the
  case a proxy fixes. Confirm your client is running and its inbound port matches what you entered.
- nothing at all: no channel is configured, and desktop notifications are switched off

Delivery failures are not silent, but they are not shown in the interface either; they are written to
the process's standard error. Launch the binary from a terminal to see them:

```sh
./src-tauri/target/release/bundle/macos/IranNANternet.app/Contents/MacOS/IranNANternet
```

## Findings reference

Every finding code and its meaning is listed on the page for the check that produces it:
[Website Monitoring](Website-Monitoring.md#the-verdict),
[Server Monitoring](Server-Monitoring.md#containers) and
[Certificates and Domains](Certificates-and-Domains.md#findings).

The translated one-line headline for each code lives in `src/i18n/en.ts` and `src/i18n/fa.ts` for the
interface, and in `src-tauri/src/notification_engine.rs` for notification text. Adding a finding means
touching both; the code itself is the key that joins them.
