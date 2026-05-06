# thorium-tabs

Resident rMenu rpack that maps Alt+mouse gestures to Thorium tab actions.

Behavior while `thorium.exe` is the foreground app:

- `Alt+WheelUp`: temporarily releases Alt, then sends `Ctrl+Tab`;
- `Alt+WheelDown`: temporarily releases Alt, then sends `Ctrl+Shift+Tab`;
- `Alt+LeftClick`: `Ctrl+W`;
- `Alt+RightClick`: temporarily releases Alt, sends `Ctrl+Shift+T`, and consumes the right-click release so the browser context menu does not open.

This module ships a native helper at `bin/thorium-tabs.exe` and declares it with `[resident]` in `module.toml`. `rmenu-daemon` starts/stops the helper; rMenu core does not implement Thorium or browser-tab behavior.

Security note: the helper uses a low-level mouse hook so it can detect Alt+mouse gestures while rMenu is closed.
