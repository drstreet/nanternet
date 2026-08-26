# IranNANternet

**English** · [فارسی](fa-Home.md)

A desktop monitor that separates a real outage from a regional block. This wiki is the full manual:
what every setting does, the exact shape of the config file, and what to do when a check reports
something you did not expect.

If you only want the pitch and the build commands, `readme.md` in the repository is shorter.

## Start here

| Page | What it answers |
| --- | --- |
| [Getting Started](Getting-Started.md) | Install it, add your first target, understand the dashboard |
| [Website Monitoring](Website-Monitoring.md) | How the Iran-access verdict is reached and why it can be trusted |
| [Server Monitoring](Server-Monitoring.md) | SSH authentication, host key pinning, thresholds, containers |
| [Certificates and Domains](Certificates-and-Domains.md) | TLS expiry, RDAP, and why `.ir` is different |
| [Notifications](Notifications.md) | Desktop, Discord, Telegram, and how alert spam is prevented |
| [Configuration Reference](Configuration-Reference.md) | Every field of `targets.json`, every default, every limit |
| [Troubleshooting](Troubleshooting.md) | Concrete symptoms and their causes |

## The one idea worth understanding

Every other feature is ordinary monitoring. This one is not.

A single-vantage-point monitor asks "does it respond?" and gets one bit back. That bit cannot tell
you whether a site is genuinely down or merely unreachable from where the monitor happens to sit.
For anything hosted on or near an Iranian network, that distinction is the whole game: a service can
be perfectly healthy and still invisible to everyone abroad, and you would never know because your
own browser loads it fine.

IranNANternet probes from three vantages at once, your own network, several Iranian ISPs, and several
countries abroad, and then reasons about the *combination*. Five distinct verdicts come out of that,
each pointing at a different thing to go fix. [Website Monitoring](Website-Monitoring.md) works through
the logic.

## Conventions in this wiki

Code, paths and identifiers appear `like this`. Values you must replace are written in angle
brackets, `<like-this>`. Shell examples assume you are in the repository root.

Findings, the individual lines a check produces, are named by their code, for example
`website.iranAccess`. Those codes are stable and appear identically in the source, the log and the
translation files, so searching for one always lands somewhere useful.
