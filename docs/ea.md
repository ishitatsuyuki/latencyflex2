---
title: Early Access Disclaimer
editLink: true
---

# {{ $frontmatter.title }}

LatencyFleX 2 is currently in the alpha stage. Keep in mind that:
- Things might be broken, and please report game compatibility issues.
- Internal APIs changes frequently. When updating builds, do it for all components at once.
- The public API is subject to change and intentionally undocumented. If you're a game developer, please wait until a stable release of LFX 2 happens.

During alpha, debug and profiling logging is always enabled. Around 1GB of data is written per hour of gameplay session. Using a filesystem with transparent compression can reduce the amount of I/O.

Finally, expect bugs since this is alpha stage software. I will not be responsible for any damages, including but not limited to broken setups or corrupted save files. Please follow backup best practices and limit the damage in case something goes wrong.

With that in mind, proceed to [Building](./shim/building.md).