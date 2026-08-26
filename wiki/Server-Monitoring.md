[Home](Home.md) · [فارسی](fa-Server-Monitoring.md)

# Server Monitoring

Server checks connect over SSH, run one command, and parse the output locally. Nothing is installed
on the server, no agent runs there, and the account needs no special privileges beyond reading
`/proc` and, if you watch containers, talking to the Docker socket.

## Authentication

Two methods are supported.

**Private key** is the recommended one, and the interface says so where you choose. The reasoning:
a stored password is a credential that can be replayed by anything that gets hold of it, while a key
never leaves your machine, and a server configured with `PasswordAuthentication no` cannot be
attacked through this path at all.

Give the path to your private key. `~` is expanded. RSA, ECDSA and Ed25519 keys all work. If the key
is encrypted, put its passphrase in the secret field; otherwise leave that field empty.

**Password** exists because sometimes you inherit a box you do not control. When you select it the
interface shows a caution note rather than hiding the tradeoff.

Either way, the secret goes to the operating system credential store, never to the config file. See
[Configuration Reference](Configuration-Reference.md#secrets) for the exact keys used.

### Editing a stored secret

When you reopen a server that already has a secret, the field is empty and a note explains why:

- leave it empty and the stored secret is kept untouched
- type a new value and it replaces the old one
- type a single space and save, and the stored secret is erased

That last one is the deliberate escape hatch for switching a server from password to key without
leaving a dead password in your keychain.

## Host key pinning

The first successful connection to a server records its host key fingerprint, SHA256, into the
target's configuration. That first connection is trust on first use: there is nothing yet to compare
against, so the key is accepted and a `server.hostKeyLearned` warning tells you it happened. Check it
against what your provider shows if the server is one you care about.

Every later connection compares the presented key against the stored one. A mismatch produces
`server.hostKeyMismatch`, critical, and the connection is refused before any credential is offered.
The detail line shows both fingerprints.

That finding has two possible causes and you must decide which:

- the server was rebuilt, or its host keys were regenerated, which is routine after a reinstall
- something is intercepting the connection

If it is the first, edit the target: saving it clears the pin, and the next connection learns the new
key. If you cannot account for the change, do not clear it. Investigate.

## What gets measured

One command runs per check, assembled from these pieces, with the Docker section included only when
you have listed containers:

```sh
echo '::load';   cat /proc/loadavg
echo '::cores';  nproc || grep -c '^processor' /proc/cpuinfo
echo '::uptime'; cat /proc/uptime
echo '::memory'; cat /proc/meminfo
echo '::disk';   df -Pk
echo '::docker'; docker ps --all --format '{{.Names}}|{{.State}}|{{.Status}}'
```

Batching it into one command matters: each SSH round trip on a bad link costs real time, and five
separate commands would make a check five times more likely to time out partway.

Every source here is a plain file in `/proc` or a POSIX-standard tool, which is why the check works on
minimal images without coreutils extras. Sections that produce nothing are reported as unavailable
rather than as zero.

### Load

`/proc/loadavg` gives the one-minute average, which is then divided by the core count. Comparing raw
load between a 2-core VPS and a 32-core machine is meaningless; per-core load is comparable, and
around 1.0 means the CPU is saturated whatever the machine.

The threshold, **Load limit per core**, defaults to 1.5. Above it you get `server.loadHigh` as a
warning. Above three times it, the same finding becomes critical.

### Memory

Read from `/proc/meminfo`, preferring `MemAvailable` because that is the kernel's own estimate of what
a new workload could actually claim. `MemFree` alone would make every healthy Linux box look full,
since the kernel deliberately uses spare memory for page cache. Where `MemAvailable` is absent, on
older kernels, the fallback is free + buffers + cached.

**Memory limit** is a percentage, default 85. Crossing it gives `server.memoryHigh` as a warning;
at 97% or above the same finding is critical.

### Disk

`df -Pk` in POSIX mode, one entry per mount point you listed. Usage is computed as used over
used + available, matching the capacity column `df` prints, rather than used over total. The two
differ by the reserved blocks and it is the former that tells you when writes will start failing.

Mount points are matched whole, including ones containing spaces such as `/mnt/backup volume`. A
configured mount point that `df` does not report gives `server.mountMissing`, a warning, which is
usually how you find out that a backup volume silently failed to mount after a reboot.

**Disk limit** is a percentage, default 85, escalating to critical at 97%. That escalation exists
because a full backup partition is not an inconvenience, it is silent data loss.

### Containers

Only when you have listed container names. `docker ps --all` is used rather than plain `docker ps`,
so a container that exists but exited is distinguishable from one that was never created; those are
different problems.

The output format uses `|` as a separator because a Docker container name cannot contain one, whereas
status text is free-form and could plausibly contain anything else.

Each name you listed is looked for exactly first, then by substring, so `db` matches `stack-db-1`. An
exact match always wins over a substring one: if you have both `db` and `stack-db-1`, asking for `db`
gets you `db`.

| Situation | Finding | Level |
| --- | --- | --- |
| no container matches the name | `docker.missing` | critical |
| the container exists but is not running | `docker.notRunning` | critical |
| healthcheck reports unhealthy | `docker.unhealthy` | critical |
| healthcheck is still starting | `docker.healthStarting` | warning |
| the Docker command produced nothing | `docker.unavailable` | critical |

Health state is read from the parenthesised part of the status text, `(healthy)`, `(unhealthy)` or
`(health: starting)`. Only those three are recognised, which is what keeps `Exited (137) 4 minutes ago`
from being misread as a health state.

`docker.unavailable` usually means the SSH user is not in the `docker` group. Add it, or point the
target at an account that is.

## Connection failures

Failures before any measurement produce one critical finding and nothing else:

| Finding | Meaning |
| --- | --- |
| `server.unreachable` | no TCP connection or no SSH banner within 12 seconds |
| `server.authenticationFailed` | the server rejected the credentials, or the key file could not be read |
| `server.hostKeyMismatch` | the host key does not match the pinned one |
| `server.probeFailed` | connected and authenticated, but the command failed |

`server.authenticationFailed` also covers an unreadable or wrong-passphrase key file, with the path
and the underlying error in the detail line.

## Timeouts

Connecting, including the SSH banner exchange, gets 12 seconds. Running the probe command gets 25.
These are generous on purpose: a link that is bad enough to make monitoring urgent is also a link
where a tight timeout produces nothing but false alarms.

## The healthy case

When nothing crossed a threshold, one `server.healthy` finding summarises what was measured, along
these lines:

```
load 0.13/core, memory 62%, / 41%, 6/6 containers up
```

That line is worth glancing at even when it is green, because it is where you notice a disk climbing
a few percent a week before it ever crosses your threshold.
