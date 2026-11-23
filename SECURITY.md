# Security policy

Naht runs a daemon on localhost that reads and writes your project files, so a
flaw in it can matter. Please report vulnerabilities privately.

## Reporting

Open a private advisory at
<https://github.com/vskstudio/naht/security/advisories/new>. Do not open a
public issue or pull request for a vulnerability.

Include the version, the steps to reproduce, and the impact you see. A
maintainer will answer within a week.

## Threat model

The gate on every route stops browsers and DNS rebinding. It refuses requests that carry an `Origin`
header, a `Host` other than the loopback names of the daemon's port, or no `x-naht-client` header, so a
web page in the user's browser cannot read or write project files through the daemon. Every route except
`GET /info` also needs the project token in the `x-naht-token` header. The token is 32 random bytes in
`.naht/token`. On Unix the file is created with owner-only permissions (mode 0600), and an existing file that
its group or others can read is tightened to 0600 when the daemon loads it, so another local account cannot
read it and cannot call the daemon. On Windows the file inherits the access rules of the `.naht` directory,
and Naht adds no further restriction. When the project sets `serve.place_id`, requests from a session bound to another place are
refused, which guards against syncing source into the wrong game. Writes are limited to paths that map to
Studio scripts or terrain blobs, and request bodies and concurrency are bounded.

The gate is not authentication against the account that runs the daemon. A process running as that account
can read `.naht/token`, and it can read and write the project files directly, so the operating system account
is the boundary that protects the project. The place id is a value the client sends, so it is not a credential;
the token is the only credential the daemon checks.

The plugin sends the token only to a daemon that proves it holds it. Each handshake sends a fresh nonce to
`GET /info` without the token, and the daemon answers with the HMAC-SHA256 of that nonce and its port, keyed by the
token. The plugin computes the same value with the token it stored and sends the token only when both match, so a
process that binds the port while `naht serve` is stopped never receives it. The port is part of the proof, so a
process on the plugin's port cannot relay the nonce to a daemon serving another port. Until a handshake is proven
the plugin sends nothing past `GET /info`: no inventory, no push, no poll, so such a process cannot feed patches
into Studio either, and a plugin with no token stored for the project id it is told stops and asks for one. A
token the user types is saved only once a daemon has proven it. A transport failure drops the proof, and the
plugin holds back every other request until the next handshake proves it again.

## Known risks

- **A daemon swapped between two requests.** Between the end of a proven handshake and the first failure the
  plugin sees, a request can still reach a process that took the port in that instant. The daemon would have to
  exit and the other process bind the port before the plugin's next request, which already fails with a refused
  connection when nothing listens. Details are in [`docs/architecture.md`](docs/architecture.md), section 11.

## Supported versions

Only the latest release gets security fixes.
