# herdr-nvim-aware

Nvim-aware keybindings for [herdr](https://herdr.dev). When Neovim owns the
focused pane, keys are forwarded to it. Otherwise herdr performs the action
(navigate, split, close, zoom).

Replaces [herdr-nvim-nav](https://github.com/aimdevlee/herdr-nvim-nav) with a
superset: navigation **plus** splits, close, quit, zoom, and scroll.

## How it works

Uses the same marker-file detection as herdr-nvim-nav:

1. Neovim writes its PID to `$XDG_CACHE_HOME/herdr/nvim-panes/<pane-id>` on
   startup and removes it on exit.
2. This plugin's C binary checks the marker on each keystroke. If a live
   Neovim owns the pane, it forwards the key via `pane.send_keys`. Otherwise
   it calls the appropriate herdr method (`pane.focus_direction`, `pane.split`,
   `pane.close`, `pane.zoom`).

The Neovim side requires a Navigator.nvim herdr backend (or herdr-nvim-nav's
lua module) to create the marker and handle edge-crossing back to herdr.

## Install

```sh
herdr plugin install KoalaVim/herdr-nvim-aware
```

## Configure

In `~/.config/herdr/config.toml`:

```toml
[keys]
focus_pane_left = ""
focus_pane_down = ""
focus_pane_up = ""
focus_pane_right = ""
split_vertical = ""
split_horizontal = ""
close_pane = ""
zoom = ""

[[keys.command]]
key = "ctrl+h"
type = "plugin_action"
command = "herdr-nvim-aware.left"

[[keys.command]]
key = "ctrl+j"
type = "plugin_action"
command = "herdr-nvim-aware.down"

[[keys.command]]
key = "ctrl+k"
type = "plugin_action"
command = "herdr-nvim-aware.up"

[[keys.command]]
key = "ctrl+l"
type = "plugin_action"
command = "herdr-nvim-aware.right"

[[keys.command]]
key = "alt+e"
type = "plugin_action"
command = "herdr-nvim-aware.split_v"

[[keys.command]]
key = "alt+o"
type = "plugin_action"
command = "herdr-nvim-aware.split_h"

[[keys.command]]
key = "alt+w"
type = "plugin_action"
command = "herdr-nvim-aware.close"

[[keys.command]]
key = "alt+q"
type = "plugin_action"
command = "herdr-nvim-aware.quit"

[[keys.command]]
key = "alt+z"
type = "plugin_action"
command = "herdr-nvim-aware.zoom"

[[keys.command]]
key = "ctrl+u"
type = "plugin_action"
command = "herdr-nvim-aware.scroll_up"
```

## Actions

| Action | Key | Nvim: sends | No nvim: herdr action |
|--------|-----|-------------|----------------------|
| `left` | `ctrl+h` | `ctrl+h` | focus pane left |
| `down` | `ctrl+j` | `ctrl+j` | focus pane down |
| `up` | `ctrl+k` | `ctrl+k` | focus pane up |
| `right` | `ctrl+l` | `ctrl+l` | focus pane right |
| `split_v` | `alt+e` | `alt+e` | split right |
| `split_h` | `alt+o` | `alt+o` | split down |
| `close` | `alt+w` | `alt+w` | close pane |
| `quit` | `alt+q` | `alt+q` | close pane |
| `zoom` | `alt+z` | `alt+z` | zoom toggle |
| `scroll_up` | `ctrl+u` | `ctrl+u` | passthrough ctrl+u |

## Requirements

- herdr >= 0.7.0
- Neovim with a herdr-aware navigation plugin (e.g. Navigator.nvim herdr backend)
- C compiler (cc/clang/gcc) for the build step

## License

[MIT](LICENSE)
