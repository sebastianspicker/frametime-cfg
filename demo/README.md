# Browser demonstration

This directory contains a dependency-free, static demonstration of the
Frametime CFG workflow. It uses sanitized fixture data and never runs
PowerShell, inspects the host, contacts a service, or writes suite state.

Open `index.html` directly in a browser, or serve the repository root with any
local static-file server and navigate to `/demo/`. The page links to repository
documentation with paths relative to the checked-out source tree.

The seven views mirror the desktop information architecture: Overview, Assess,
Setup, Benchmark, Network, Video, and Recovery. Buttons update browser-local UI
state only. The setup command shown in the demo is documentation, not an
executable browser action.

Run the dependency-free checks from the repository root:

```console
node --check demo/app.js
node --test demo/demo.test.mjs
```

The checks validate the complete static entrypoint, navigation hooks, local-only
content-security policy, documentation links, responsive and focus contracts,
and JavaScript syntax. Browser verification remains required for layout and
interaction changes.
