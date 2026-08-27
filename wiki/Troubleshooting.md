[Home](Home.md) · [فارسی](fa-Troubleshooting.md)

# Troubleshooting

Organised by what you observe, since that is what you have when something goes wrong.

## Seeing what the app is thinking

Delivery failures, persistence errors and check errors are written to standard error. The interface
does not show them, because a notification about a failed notification is not much use. Launch the
binary from a terminal to read them:

```sh
# macOS
./src-tauri/target/release/bundle/macos/IranNANternet.app/Contents/MacOS/IranNANternet

# during development, with the frontend dev server
npm run app
```

## Websites

### Everything reports `website.externalProbeFailed`

check-host.net is refusing you, almost always for rate limiting. Raise **External probe every** and
stop pressing **Check now**. The detail line quotes what check-host actually said.

If it says the request was refused rather than timed out, wait a few minutes; the limit is per client
and it clears.

### A site is up but reports `website.iranAccess`

That is the feature working, and it is worth confirming before you doubt it. Expand the row and read
the vantage point list: if every foreign node reports a timeout while the Iranian ones report 200,
your site really is unreachable from outside. Open the check-host report link for the raw upstream
view.

The rare false positive comes from configuring only one or two foreign nodes and having them land in a
region with a transient problem. Raise **Vantage points abroad** to 4 or more; nodes are dealt out one
per continent, so a higher number really does mean more independent evidence. If a particular site
always loses the same node, raise **Warn when this many foreign nodes fail** instead of turning the
external probe off.

### A site is down but reports healthy

The local request succeeded, so something answered. Two common causes:

- your ISP or a captive portal returned its own page with status 200. Set **Text that must appear on
  the page** to something only your real site serves.
- the site answers on `/` but the thing you care about is broken elsewhere. Point the target at the
  specific URL, a health endpoint if you have one.

### `website.unexpectedStatus` on a healthy site

Either you set **Expected status code** and the server legitimately answers something else, a 301 to
`www` being the usual one, or you left it empty and the server is answering 400 or above. Point the
target at the final URL or set the expected code to what it actually returns.

### Every node stays grey

Nodes that had not answered within the 28 second polling budget stay unknown. Occasional greys are
normal; a slow node is not evidence about your site, which is why they are not counted as failures.
All nodes grey, every time, means check-host is not running your job at all, which is the rate limit
again.

## Servers

### `server.unreachable`

No TCP connection or no SSH banner within 12 seconds. Verify from a terminal:

```sh
ssh -v -p <port> <user>@<host>
```

If that also hangs, the problem is the network or the firewall, not the app. If it connects instantly,
check that the port in the target matches.

### `server.authenticationFailed`

The detail line distinguishes the cases.

A message about the key path means the file could not be read or decrypted. Confirm the path, and if
the key is encrypted, that its passphrase is stored. Note that the passphrase field being empty on
reopening a target is normal and does not mean it was lost.

A message about rejected credentials means the server said no. Confirm with `ssh -v` and check whether
the server allows the method you chose; a server with `PasswordAuthentication no` will refuse a
password no matter how correct it is.

On a headless Linux machine with no Secret Service provider, secrets cannot be stored at all, so
password-authenticated targets always fail here. Use an unencrypted key instead.

### `server.hostKeyMismatch`

The host key changed. Two possibilities, and you decide which:

- the server was reinstalled or its host keys were regenerated, which is routine
- something is intercepting the connection

Compare the fingerprint in the detail line with what the server actually presents:

```sh
ssh-keyscan -p <port> <host> | ssh-keygen -lf -
```

If it matches and you can account for the change, edit the target and save; that clears the pin and the
next connection learns the new key. If you cannot account for it, do not clear it.

### `docker.unavailable`

The SSH user cannot talk to the Docker socket. Add it to the group:

```sh
sudo usermod -aG docker <user>
```

Then reconnect, since group membership applies to new sessions. Confirm with
`ssh <user>@<host> docker ps`.

### `docker.missing` for a container that is running

Names are matched exactly first, then by substring. If the running container is `stack-db-1`, both
`db` and `stack-db-1` find it. If neither does, compare against the real output:

```sh
ssh <user>@<host> docker ps --all --format '{{.Names}}'
```

Watch for a different Docker context or a rootless daemon, where the SSH user sees a different set of
containers than you do interactively.

### `server.mountMissing`

`df` on the server does not report that mount point. This is frequently a real finding: a backup
volume that failed to mount after a reboot. Check the actual list:

```sh
ssh <user>@<host> df -Pk
```

Mount points must match whole and exactly, including case. Spaces are fine, `/mnt/backup volume`
works, but a trailing slash where `df` has none will not match.

### Memory sits at 80% or more on an idle server

That is usually correct rather than a bug. Linux deliberately fills unused memory with page cache, so
`MemFree` on a healthy machine is always small. The reading here uses `MemAvailable`, the kernel's own
estimate of what a new workload could claim, which is the honest number. If it is genuinely high,
`ssh <user>@<host> free -h` will agree.

## Certificates and domains

### `certificate.unavailable` on a site that works in a browser

Check the detail line. "No TCP connection" points at the network, and on a degraded link DNS
resolution alone has been measured taking 9 to 12 seconds for a name that resolves in 10 milliseconds
an hour later. Retry before concluding anything.

"Did not finish the TLS handshake" points at TLS itself, and the most common cause is a certificate
that has already expired or does not match the hostname. Validation is left on, so an invalid
certificate fails the handshake rather than reporting a precise date; that is the tradeoff described
in [Certificates and Domains](Certificates-and-Domains.md#tls-certificates).

### `domain.registrationUnpublished` on a `.ir` domain

Working as intended, and not a warning. IRNIC filters expiry dates out of its whois output and there is
no `.ir` RDAP service, so the date cannot be read by any tool. Turn **Check the domain registration**
off for that target if you would rather not see the line at all.

### `domain.lookupFailed`

RDAP did not answer, or answered without an expiration event. Some registries publish no expiry over
RDAP at all; in that case the finding is permanent and turning the registration check off for that
target is the right response.

Because lookups run twice a day, a transient failure persists in the interface until the next
scheduled attempt. **Check now** forces one immediately.

## The app itself

### Notifications never arrive

Work through [Notifications](Notifications.md#testing-it), which lists the causes by which channel is
affected.

### Checks stop when the window is closed

They should not; closing hides to the tray. If they really stopped, the app was quit. Enable **Start
when I log in**, and use the tray icon rather than Cmd-Q out of habit.

### Nothing happens after adding a target

Every target is checked immediately when added, so a result should appear within seconds. If it stays
unknown, the target is probably paused; check the toggle. Otherwise run from a terminal and look for a
`check failed` line.

### The interface is in the wrong language

Language is a setting, not a system-locale detection, and it defaults to English. Change it in
Settings; it applies immediately to the interface, the layout direction and alert text.

## Building

### `npm install` fails with `ETIMEDOUT`

Some Iranian connections stall partway through a large package document even though `curl` to the same
registry works. Use a mirror:

```sh
npm install --registry=https://registry.npmmirror.com
```

Do not commit an `.npmrc` containing it; that file is gitignored for this reason.

### `cargo build` cannot reach crates.io

`https://crates.io` may answer 403 from some regions, but that host is only the website and the API.
Cargo itself uses `index.crates.io` and `static.crates.io`, which generally remain reachable. If the
build works, the 403 you saw in a browser is irrelevant.

### `tauri build` fails on a missing icon

Icons are generated files. Regenerate them:

```sh
python3 tools/make-icon.py tools/icon.png
npx tauri icon tools/icon.png
```

### The debug binary opens a blank window

Expected. A debug build loads the dev server URL, so it needs Vite running. Use `npm run app`, which
starts both, rather than running `target/debug/irannanternet` on its own.
