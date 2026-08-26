[Home](Home.md) · [فارسی](fa-Certificates-and-Domains.md)

# Certificates and Domains

A domain target watches two unrelated clocks that both end in an outage nobody noticed coming: the
TLS certificate and the domain registration. Either can be turned off independently.

## TLS certificates

The check opens a real TLS connection to the host and port and reads the certificate the server
actually presents. It does not query an API or a transparency log, because those tell you about a
certificate that was issued, not about the one your visitors are being served. Those differ every
time a renewal succeeds but the reload does not.

From the leaf certificate three things are taken: the subject common name, the issuer common name,
and `notAfter`. Days remaining is computed from `notAfter` against the current time.

Certificate validation is left on. That is a deliberate choice with a consequence worth knowing: a
certificate that has already expired, or is self-signed, or does not match the hostname, will fail
the handshake, so you get `certificate.unavailable` rather than a precise expiry number. Since the
point of the feature is to warn you *before* expiry, while the certificate is still valid and the
handshake still succeeds, this trades a rare diagnostic for a great deal less code. When the
handshake does fail you still get a critical finding with the underlying reason in the detail line.

### Severity

| Days remaining | Finding | Level |
| --- | --- | --- |
| more than the warning window | `certificate.valid` | healthy |
| within the window, above a third of it | `certificate.expiring` | warning |
| within a third of the window | `certificate.expiring` | critical |
| zero or negative | `certificate.expired` | critical |

**Warn this many days ahead** defaults to 14 and is clamped between 1 and 120. With the default,
you get a warning at 14 days and it turns critical at 4. The escalation exists because a warning you
have already seen ten times stops registering, whereas a change in level gets a fresh notification.

For a Let's Encrypt certificate on a 90 day cycle, renewal happens around day 60, so 14 days is
already deep into "something is wrong with your renewal" territory rather than a routine reminder.

### Timeouts

Connecting gets 20 seconds and the handshake gets 15, reported separately so you can tell a routing
problem from a TLS problem. These are longer than they look like they should be, for a specific
reason: on a degraded connection DNS resolution alone has been measured taking 9 to 12 seconds for
the same name that resolves in 10 milliseconds an hour later. A tighter budget produces
`certificate.unavailable` findings that are about your link, not about the certificate.

## Domain registration

Registration expiry comes from [RDAP](https://about.rdap.org/), the protocol that replaced whois for
most registries. It is a JSON API, which means the expiry date arrives as a labelled field rather
than as free-form text to be scraped.

The lookup goes through `rdap.org`, a redirector that forwards to whichever registry serves the
domain, so no bootstrap table has to be maintained. The response's event list is searched for
`expiration` and its date parsed as RFC 3339, falling back to a plain `YYYY-MM-DD` date.

Severity works exactly like certificates, with `domain.valid`, `domain.expiring` and `domain.expired`.

The registrable domain is derived before looking up, so `panel.example.com` is looked up as
`example.com`, since a subdomain has no registration of its own. The derivation is a short heuristic
on label lengths rather than a full public suffix list: it handles `example.co.uk`, `example.co.ir`
and ordinary two-label domains correctly, but it is not exhaustive and an unusual multi-label suffix
may be split wrong. Replacing it with a real public suffix list means taking on a dependency and a
data file that needs refreshing, which has not been worth it so far. If it gets a domain of yours
wrong, that is a bug report worth filing.

### Cadence

Registration is looked up at most once every 12 hours, and the result is carried forward between
lookups with the day count recalculated from the stored date so the warning stays accurate as time
passes.

This is not an optimisation, it is a requirement. The date changes once a year, so asking every two
minutes gains nothing, and registries throttle or block clients that do. During development, the
`.ir` whois server stopped answering entirely after a handful of queries in a row.

Failures are carried forward too. Without that, a failed lookup would show a warning on one cycle and
disappear on the next, and the app would alternate between warning and recovery notifications every
twelve hours.

**Check now** forces a fresh lookup, ignoring the 12 hour clock. Use it after a renewal; do not sit
on it.

## Why .ir is different

For a `.ir` domain the registration expiry is not obtainable. Not "hard to get", not "needs an API
key". It is not published.

IRNIC's whois server, version 2.0.0, returns an answer that says `This output has been filtered` and
contains no date field at all. This is true for every `.ir` domain, including IRNIC's own:

```
% This is the IRNIC Whois server v2.0.0.
% NOTE: This output has been filtered.
% Information related to 'nic.ir'
domain:     nic.ir
ascii:      nic.ir
nserver:    ns1.nic.ir
source:     IRNIC
```

There is also no `.ir` RDAP service: `rdap.org/domain/nic.ir` answers 404 and `api.nic.ir` answers
403.

An earlier version of this app parsed an `expire-date` field from IRNIC. That field no longer exists,
so the code was deleted rather than left in to fail forever against a rate-limited server.

What you get instead is a healthy-level finding, `domain.registrationUnpublished`, stating plainly
that the registry does not publish the date. It is not a warning, because nothing is wrong; the
information simply does not exist. The certificate check on the same target runs normally.

If you need to track a `.ir` renewal date, put a calendar reminder next to whatever your registrar's
panel tells you. No tool can read it for you.

## Findings

| Finding | Level | Meaning |
| --- | --- | --- |
| `certificate.valid` | healthy | comfortably in date |
| `certificate.expiring` | warning or critical | inside the warning window |
| `certificate.expired` | critical | past `notAfter` |
| `certificate.unavailable` | critical | connection, handshake or parsing failed |
| `domain.valid` | healthy | comfortably in date |
| `domain.expiring` | warning or critical | inside the warning window |
| `domain.expired` | critical | past the registry's expiry date |
| `domain.lookupFailed` | warning | RDAP did not answer or published no date |
| `domain.registrationUnpublished` | healthy | the registry does not publish expiry, as with `.ir` |
| `domain.invalid` | critical | the configured name is not a plain domain name |

`domain.invalid` is a validation result, not a network one. The domain is checked for shape before any
socket opens: ASCII letters, digits, dots and hyphens only, no leading or trailing dot, at most 253
characters. This rejects anything carrying control characters, which is the sort of input that turns
a text-protocol client into a request-splitting bug.
