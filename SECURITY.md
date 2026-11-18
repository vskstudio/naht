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
`.naht/token`, created with owner-only permissions, so another local account cannot read it and cannot call
the daemon. When the project sets `serve.place_id`, requests from a session bound to another place are
refused, which guards against syncing source into the wrong game. Writes are limited to paths that map to
Studio scripts or terrain blobs, and request bodies and concurrency are bounded.

The gate is not authentication against the account that runs the daemon. A process running as that account
can read `.naht/token`, and it can read and write the project files directly, so the operating system account
is the boundary that protects the project. The place id is a value the client sends, so it is not a credential;
the token is the only credential the daemon checks.

## Supported versions

Only the latest release gets security fixes.
