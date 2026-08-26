[Home](Home.md) · [فارسی](fa-Getting-Started.md)

# Getting Started

## Requirements

- Node 20 or newer
- A Rust toolchain, install it from [rustup.rs](https://rustup.rs)
- The [Tauri system prerequisites](https://v2.tauri.app/start/prerequisites/) for your platform

On macOS that means Xcode command line tools. On Debian and Ubuntu it means the WebKitGTK
development packages; the exact list is in `.github/workflows/ci.yml` if you would rather copy it
than read the Tauri docs.

## Build and run

```sh
npm install
npm run app          # development window with hot reload
npm run app:build    # release bundle for the current platform
```

`npm run app:build` writes into `src-tauri/target/release/bundle/`. On macOS you get an `.app` and a
`.dmg`, on Windows an `.msi` and an `.exe`, on Linux a `.deb`, an `.rpm` and an `.AppImage`.

Cross-compiling desktop bundles does not really work. To publish for all three platforms, push a tag
and let the `release` workflow build each one on its native runner.

### If npm cannot reach the registry

On some Iranian connections `npm install` stalls partway through a large package document and
eventually fails with `ETIMEDOUT`, even though `curl` to the same registry works. A mirror gets
around it:

```sh
npm install --registry=https://registry.npmmirror.com
```

Do not commit an `.npmrc` with that mirror. It is already in `.gitignore` for exactly this reason:
the workaround belongs to your network, not to the project.

## Adding your first target

Press **Add target** and pick one of three kinds. The choice is fixed after creation, because the
three kinds have almost nothing in common; to change it, delete and re-add.

### A website

The minimum is a name and an address. Everything else has a working default.

Leave **Also probe from outside** on. Without it you get an ordinary uptime monitor and lose the one
thing this app exists for. With it on you also get **External probe every**, which stays deliberately
slower than the main interval because check-host.net limits how often you may ask.

**Expected status code** left empty means anything below 400 is fine. Set it when you are watching
something that must answer exactly 200 and where a 301 would mean a misconfiguration.

**Text that must appear on the page** catches the failure mode where the server answers 200 with a
"database unavailable" page. Keep the string short and pick something a broken render would drop, a
word from your footer rather than from a heading.

### A server

Host, port, SSH user, then authentication.

Choose **Private key** if you can. The app says so in the interface too, and the reason is worth
stating plainly: a password stored anywhere is a password that can be replayed, whereas a key never
leaves your machine and can be refused entirely at the server with `PasswordAuthentication no`.

Point the key path at your private key, `~/.ssh/id_ed25519` or similar. If that key is encrypted,
put its passphrase in the secret field; if it is not, leave the field empty.

The first successful connection records the server's host key fingerprint. From then on a changed
fingerprint is reported as critical rather than accepted. See
[Server Monitoring](Server-Monitoring.md#host-key-pinning) for what to do when that fires.

**Containers that must stay up** takes one name per line. A plain name also matches Compose
suffixes, so `db` finds `stack-db-1`. Leave it empty and Docker is not touched at all, which also
means the check runs one command less.

### A domain

A domain name is all that is required. It watches two independent things, the TLS certificate and the
registration, and either can be turned off.

For a `.ir` domain, turn **Check the domain registration** off or ignore its result: IRNIC does not
publish expiry dates. [Certificates and Domains](Certificates-and-Domains.md#why-ir-is-different)
explains why, and the app tells you the same thing instead of pretending to fail.

## Reading the dashboard

Four counters across the top summarise every enabled target by severity. Paused targets count as
not yet checked, since a stale verdict is not a verdict.

Each row shows the target name, its kind, the endpoint, when it was last checked and its current
level. Click the row to expand it.

Expanded, a row leads with its findings. Each finding is one sentence naming the condition and one
line of raw detail underneath, so you can see both the interpretation and the evidence. Below that
comes whatever the check measured: vantage point results for a website, meters and containers for a
server, dates for a domain.

The severity levels mean:

| Level | Meaning |
| --- | --- |
| healthy | every check passed |
| warning | something needs attention but the service works |
| critical | the service is broken, unreachable, or about to be |
| unknown | no check has completed yet |

A target's level is the worst of its findings. One critical finding among five healthy ones makes the
target critical.

## Letting it actually run

A desktop monitor only monitors while it runs. Two settings matter:

- Closing the window hides it to the tray instead of quitting. This is the default and cannot be
  turned off, because a monitor that quits on window close is a monitor that lies to you.
- **Start when I log in**, in Settings, registers a login item so checks resume after a reboot.

To quit properly, use **Quit** in the tray menu.
