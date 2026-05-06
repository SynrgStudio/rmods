# taskbar-volume

Resident rMenu rpack that controls Windows system volume from the taskbar.

Behavior:

- mouse wheel up over the Windows taskbar: volume up;
- mouse wheel down over the Windows taskbar: volume down;
- middle click over empty Windows taskbar background: mute toggle.

Middle click over a taskbar application icon is passed through to Windows so it can keep opening a new app window.

This module ships a native helper at `bin/taskbar-volume.exe` and declares it with `[resident]` in `module.toml`. `rmenu-daemon` starts/stops the helper; rMenu core does not implement taskbar or volume behavior.

Security note: the helper uses a low-level mouse hook so it can detect taskbar mouse gestures while rMenu is closed.
