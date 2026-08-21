# Browser demonstration

This directory contains a dependency-free, static demonstration of the
Frametime CFG workflow. It uses illustrative local data and never runs
PowerShell, inspects the host, contacts a service, or writes suite state.

Open `index.html` directly in a browser, or serve the repository root with any
local static-file server and navigate to `/demo/`. The page links to repository
documentation with paths relative to the checked-out source tree.

The seven views mirror the desktop information architecture: Overview, Assess,
Setup, Benchmark, Network, Video, and Recovery. Buttons update browser-local UI
state only. The setup command shown in the demo is documentation, not an
executable browser action.

Check its JavaScript syntax from the repository root:

```console
node --check demo/app.js
```

Browser verification remains required for layout and interaction changes.
