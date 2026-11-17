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
web page in the user's browser cannot read or write project files through the daemon. When the project
sets `serve.place_id`, requests from a session bound to another place are refused, which guards against
syncing source into the wrong game. Writes are limited to paths that map to Studio scripts or terrain
blobs, and request bodies and concurrency are bounded.

The gate is not authentication. The daemon trusts any process on the machine that can reach `127.0.0.1`
and sends the headers, which takes no more than a `curl` call. Any program running as a user can read and
write that user's project through the daemon, and on a machine shared with other accounts, any local
account that can reach the port has the same access. The operating system account is the boundary that
protects the project. A per-daemon secret or a peer-credential check on a Unix socket would close this gap
for shared machines; neither is implemented yet (see the follow-ups in `docs/architecture.md`).

## Supported versions

Only the latest release gets security fixes.
