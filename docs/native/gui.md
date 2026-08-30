# Native GUI

`frametime-gui.exe` is a native Windows desktop entrypoint. It uses the same application layer as the CLI and displays package-authentication state before exposing protected operations.

The GUI does not make an unauthenticated package trusted and does not bypass command, phase, recovery, or elevation checks. When authority is unavailable, it remains a read-only or unavailable surface as appropriate.

Host builds and model tests do not establish keyboard navigation, screen-reader output, High Contrast, scaling, window behavior, elevation, or live Windows integration. Record UI Automation and manual evidence on Windows for changes to those areas.
