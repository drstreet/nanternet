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

The message is posted with the title in bold and the findings below. Content is capped at 1900
characters, under Discord's limit, with an ellipsis if it would run over.

Treat the URL as a secret. Anyone holding it can post to that channel. It lives in the config file
rather than the keychain, because unlike a password it is a location as much as a credential, and
splitting it would make the config file unable to describe where alerts go.

### Telegram

Two values are needed.

The **bot token** comes from [@BotFather](https://t.me/botfather): send `/newbot`, follow the prompts,
and copy the token, which looks like `123456789:AAH...`. This is a real credential and goes to the
system keychain, not to the config file.

The **chat ID** is where messages go. For a private chat with yourself, message
[@userinfobot](https://t.me/userinfobot) to get your numeric ID. For a channel or group, add the bot
as a member first, then use the numeric ID, which for channels starts with `-100`.

Messages are sent as plain text with link previews disabled, capped at 3900 characters. Plain text
rather than HTML or Markdown is deliberate: finding details contain arbitrary strings such as error
messages and container names, and a stray `<` or `_` in one of those would make Telegram reject the
whole message. Losing an alert to a formatting error is not a trade worth making.

Note that Telegram's API is not always reachable from Iranian networks without a proxy. The app
respects `HTTPS_PROXY` for outbound notification and check-host traffic, while the local website
probe deliberately ignores it so that it keeps measuring your real network.

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
