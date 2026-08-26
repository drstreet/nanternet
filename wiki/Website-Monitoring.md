[Home](Home.md) · [فارسی](fa-Website-Monitoring.md)

# Website Monitoring

This is the part of the app that does something a normal uptime monitor cannot. It is worth
understanding rather than trusting.

## The problem with one vantage point

A conventional monitor requests your URL from wherever it happens to run and reports the result. That
gives you one bit of information, and one bit cannot distinguish these four situations:

1. The server is down.
2. The server is fine, but unreachable from outside Iran.
3. The server is fine, but unreachable from inside Iran.
4. The server is fine, but your own ISP cannot route to it.

All four look identical from a single point, yet each needs a completely different response. Case 1
is your problem to fix. Case 2 usually means an inbound routing or filtering change. Case 3 often
means the host got caught in a block. Case 4 means nothing is wrong with the service at all.

If you monitor from inside Iran, case 2 is invisible: the check passes, your browser agrees, and your
users abroad silently disappear. That is the failure this app was built for.

## How a check actually runs

Each cycle does two things.

**The local request** goes out from your machine through your real network, deliberately ignoring any
`HTTP_PROXY` environment variable. If it went through your proxy it would answer a different
question, one about your proxy rather than your network. Redirects are followed. The status code and
round-trip time are recorded, and, if you configured one, the response body is searched for your
expected string.

**The external probe** goes to [check-host.net](https://check-host.net), which runs the same HTTP
request from nodes around the world. This is a two-step API: one request starts the job and returns
an identifier, then results are polled until every node has answered or a 28 second budget runs out.
Nodes that never answer stay marked unknown rather than being counted as failures, because a
check-host node being slow is not evidence about your site.

## Node selection

Around 58 nodes are available, roughly 8 of them in Iran. Rather than take whatever comes first, one
node is picked per network, deduplicating Iranian nodes by ASN and foreign nodes by country.

This is the difference between a useful signal and an average. Three Iranian nodes on the same ASN
tell you about one operator three times. Three nodes on three ASNs tell you whether the problem is
your host or one operator's routing. A real selection looks like this:

```
ir1.node.check-host.net   ir  Tehran     AS47430
ir2.node.check-host.net   ir  Isfahan    AS209279
at1.node.check-host.net   at  Vienna
bg1.node.check-host.net   bg  Sofia
br1.node.check-host.net   br  Sao Paulo
```

The node list is fetched once and cached for six hours.

## Why an HTTP error counts as reachable

A node result carries both a success flag and a status code. When the status code is present, the
server answered, and that is what reachability means, regardless of whether it answered 200 or 404 or
500.

This matters more than it sounds. Treating a 404 as unreachable would produce a spurious
`website.iranAccess` verdict for every path that happens to be missing from abroad, and that is
exactly the kind of false alarm that trains people to ignore alerts. A wrong status code is reported
separately as `website.unexpectedStatus`, a warning, because it is a different problem.

## The verdict

With a local result and two groups of node results, the classification runs in order. The first
matching rule wins and no further findings are added, because once you know the site is Iran-access
there is no value in also being told that two nodes were slow.

| Local | Abroad | Inside Iran | Finding | Level |
| --- | --- | --- | --- | --- |
| reachable | all failed | — | `website.iranAccess` | critical |
| failed | some succeeded | — | `website.blockedLocally` | critical |
| — | some succeeded | all failed | `website.blockedInsideIran` | critical |
| failed | all failed or none | — | `website.down` | critical |

If none of those match, the site is reachable and the remaining checks accumulate as warnings:

| Condition | Finding |
| --- | --- |
| status code is not the one you configured | `website.unexpectedStatus` |
| no expected code set and the server answered 400 or above | `website.unexpectedStatus` |
| your expected text was missing from the body | `website.contentMismatch` |
| some but not all Iranian nodes failed | `website.ispPartial` |
| some but not all foreign nodes failed | `website.abroadPartial` |
| the external probe itself could not run | `website.externalProbeFailed` |

With no warnings at all, one `website.healthy` finding is produced carrying the status code and
latency.

### What each verdict means for you

`website.iranAccess` is the headline case. Your site works from your desk and from nowhere abroad.
Check whether your host or CDN changed anything about inbound routing, and whether your IP range
ended up on a list.

`website.blockedLocally` means everyone else can reach it and you cannot. The service is fine. Look
at your own DNS resolver and your ISP, not at your server. Switching on the DNS check below turns
that instruction into an answer.

`website.blockedInsideIran` is the reverse of the headline case: foreign nodes succeed while every
Iranian node fails. Your site is up but Iranian visitors cannot get to it.

`website.down` is the boring one. Nothing reaches it. Go look at the server.

`website.ispPartial` is the warning you asked for when you said some connections cannot reach the
server. The detail line names which vantage points failed and what error they got, so you can tell
whether it is one operator or a coincidence.

## Checking through several DNS resolvers

Off by default, per target. It answers a question the checks above cannot: when a site does not load,
is the name being answered with a lie, or is the real address blocked? Those need opposite responses,
and until you know which one you have, you are guessing.

Every check above resolves names through your operating system, so a poisoned answer and a dead TCP
port produce the same message. The distinction is not cosmetic. One is fixed by changing a setting on
your own machine; the other cannot be fixed from your machine at all.

### What it does per resolver

**Resolves the name** by sending a DNS query straight over UDP/53 to that resolver's IP. Not through
a resolver library and not over DNS-over-HTTPS, because the resolvers that matter for this question,
Shecan, 403.online and Begzar, offer no DoH. A query is A records only.

**Fetches the site through the answer** by re-running the very same HTTP request pinned to the
returned address. Not a TCP connect, a real request with a real TLS handshake, because Iran also
filters on SNI: a connect to the correct address succeeds and the handshake is then reset. A TCP
check would call that reachable and be wrong.

Both halves matter. The address alone tells you what you were told; the fetch tells you whether it
was true.

Resolvers are queried concurrently. A dozen of them timing out in sequence would take minutes and the
check would look hung, so a whole pass costs roughly one timeout, not their sum.

### The resolvers

Shecan, 403.online, Begzar and Electro are Iranian anti-sanction resolvers, and they are the ones that
reveal a local block. Cloudflare and Google are there as a reference to compare against. All six are
built in.

Add your own by IP in Settings, one per line. IPs only, and not out of pedantry: a resolver given as a
hostname would have to be resolved by the very DNS this check exists to distrust. A custom entry that
repeats a built-in is merged rather than probed twice.

Note that 403.online lives on a private address, `10.202.10.202`, reachable only from inside an
Iranian ISP. From anywhere else it times out, which is correct and costs you nothing but one grey row.

### Findings

| Condition | Finding | Level |
| --- | --- | --- |
| a resolver returned a non-routable address while another returned a real one | `website.dnsSinkholed` | critical |
| your network failed but some resolver's address serves the site | `website.dnsBypass` | warning |
| not one resolver answered | `website.dnsUnavailable` | warning |

`website.dnsSinkholed` is proof rather than inference. Iran's filtering page sits on `10.10.34.34`, a
private address, and a private address cannot be a public web server. Anything private, loopback,
link-local, carrier-grade NAT or `0.0.0.0` counts.

The qualifier matters: it fires only when a **different** resolver returned a routable address. Without
that, a site genuinely hosted on your LAN would be reported as filtered, since every resolver would
correctly answer with a private address.

`website.dnsBypass` names the resolvers that still work, so the detail line is the fix.

`website.dnsUnavailable` exists to stop a misdiagnosis. When nothing answers, the useful conclusion is
that this network blocks outbound port 53, not that your site is broken.

### What is deliberately not reported

Resolvers disagreeing about the address. A CDN hands a different edge to every resolver as a matter of
routine, and a real check confirms it:

```
Shecan       178.22.122.100   188.114.99.0     HTTP 200
Begzar       185.55.226.26    188.114.99.0     HTTP 200
Cloudflare   1.1.1.1          104.20.23.154    HTTP 200
Google       8.8.8.8          172.66.147.243   HTTP 200
```

Four resolvers, three different addresses, nothing wrong. A finding for divergence would fire on
almost every site behind a CDN, and a warning that is usually wrong is worse than no warning.

### Cost and cadence

One DNS query plus one full HTTP request per resolver, so seven resolvers means seven extra fetches of
your page. That is why it is off by default, and why it rides the slow clock rather than the main one:
it runs on **External probe every**, 15 minutes by default, not on **Check every**. Results are carried
forward in between, exactly like the check-host results. **Check now** forces a fresh pass.

## Cadence and rate limits

check-host.net limits how often you may start a check. Exceeding it gets requests refused, which
would turn into a stream of `website.externalProbeFailed` warnings telling you nothing.

So the two probes run on separate clocks. The local request follows **Check every**, minimum 15
seconds, default 2 minutes. The external probe follows **External probe every**, floored at 5 minutes
and never faster than the main interval, default 15 minutes.

Between external probes the most recent node results are carried forward and still feed the verdict.
This is deliberate: if the site is Iran-access, it stays Iran-access between probes, and the verdict
should keep saying so. The expanded row shows how old the external measurement is so you are never
misled about its freshness.

The same carrying applies to external probe failures. Without it, a failed probe would produce a
warning on one cycle and vanish on the next, and the app would alternate between "warning" and
"recovered" notifications every fifteen minutes.

**Check now** on a target forces a full check including a fresh external probe, ignoring both clocks.
Use it after making a change; do not lean on it, or check-host will start refusing you.

## Reading the vantage point list

Each node line shows its city, its ASN where known, and its result. Green means reachable, with the
status code and latency. Red means unreachable, with the error text check-host reported, usually
something like `Connection timed out` or `No such device or address`. Grey means the node had not
finished when the polling budget ran out.

Where check-host produced a permanent report link, the row offers a button to open it in your browser
for the raw upstream view.
