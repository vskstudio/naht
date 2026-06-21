# `plugin/` — reserved

The thin Luau Studio plugin lands in **Stage 6**. It long-polls the daemon, applies received patches
to the DataModel, and POSTs Studio-side edits back — strictly transport, apply, and report, with **no
sync logic** (that all lives in [`naht-core`](../naht-core)). See [`docs/spec.md`](../docs/spec.md)
Stage 6 and [`docs/architecture.md`](../docs/architecture.md) §3.
