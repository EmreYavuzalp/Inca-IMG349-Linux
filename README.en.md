# img349ctl

[Türkçe](README.md) | **English**

Configure the **Inca IMG-349** gaming mouse on Linux, without the vendor's Windows
software. Comes as a command line tool (`img349ctl`) and a graphical app (`img349ctl-gui`).

> This is a new project, tried on one mouse so far. See
> [what has been verified](#tested-on-hardware) and report problems in an
> [issue](https://github.com/EmreYavuzalp/Inca-IMG349-Linux/issues).

## Features

- 6 DPI levels (200-6400): change values and colours, switch between levels
- See the current DPI, including changes made with the mouse's DPI buttons
- LED modes: single colour, DPI colour, colourful, rainbow, floating light, waltz, seasons, off; speed and brightness
- Bind 7 buttons, separately for mode A and mode B: mouse buttons, DPI, fire, media, web, computer functions and keyboard shortcuts
- Polling rate: 125, 250, 500 or 1000 Hz
- GUI in English and Turkish, on Wayland and X11

The mouse keeps its settings in its own memory and can report them, so there is no
profile file and no background service. Media keys and shortcuts run inside the mouse too.

## Not supported yet

- Macros
- Repeat count and interval of the fire function (the 3 preset fire modes can be bound)
- "Movement speed", "double-click speed" and "scroll speed" in the vendor tool: they
  don't write to the mouse and are probably Windows settings

## Install

From the [latest release](https://github.com/EmreYavuzalp/Inca-IMG349-Linux/releases/latest),
download the `.deb`:

```sh
sudo apt install ./img349ctl_*_amd64.deb
```

Then re-plug the mouse. **IMG-349 Mouse Settings** appears in the menu.

Building from source needs Rust 1.81 or newer:

```sh
git clone https://github.com/EmreYavuzalp/Inca-IMG349-Linux
cd Inca-IMG349-Linux
cargo build --release --features gui
sudo install -m 644 contrib/70-img349-mouse.rules /etc/udev/rules.d/
sudo udevadm control --reload-rules
sudo udevadm trigger --subsystem-match=hidraw
```

## Usage

Commands that change a setting write only what differs and read it back to check.

| Command | What it does |
|---|---|
| `img349ctl status` | show the mouse and its settings |
| `img349ctl dpi LEVEL VALUE` | set a level (1-6) to 200/400/600/800/1000/1200/1600/2000/2400/3200/4800/6400 |
| `img349ctl level LEVEL` | switch to that DPI level |
| `img349ctl color LEVEL COLOUR` | `ff8000`, `#f80` or a colour name; `all` works as the level |
| `img349ctl single COLOUR` | one colour: DPI colour mode with every level set to it; the light keeps breathing |
| `img349ctl led MODE` | `dpi`, `colorful`, `rainbow`, `floe`, `waltz`, `seasons`, `off` |
| `img349ctl led --speed N` | lighting speed, 0 fastest, 7 slowest |
| `img349ctl led --half` / `--full` | brightness |
| `img349ctl rate HZ` | polling rate; the mouse disconnects for a moment and restarts |
| `img349ctl button BUTTON ACTION [--mode b]` | bind a button (1-7) |
| `img349ctl mode a` / `b` | choose the mode in use |
| `img349ctl actions` | list what buttons can do |
| `img349ctl reset` | put the settings back to factory values |
| `img349ctl backup FILE` | save the mouse's whole memory to a file |

`-n` prints the reports instead of sending them. For a shortcut use
`img349ctl button 5 keys:ctrl+c`: modifiers and one other key.

## Safety

The mouse's memory also holds its firmware, so a wrong write could break it. The tool
writes only to the mapped settings addresses and refuses to write if the memory doesn't
look as expected. The left button of mode A cannot be changed, as in the vendor tool.
There is still no warranty; use it at your own risk.

"Factory settings" are the values read from a new mouse. What the vendor tool's reset
button writes has not been examined.

## Tested on hardware

**Verified (written and read back):** DPI value and level, colours, LED mode, speed and
brightness, button bindings, mode A/B, polling rate.

**The mouse can't do:** a steady light. With a single colour the light still breathes; the firmware accepts no mode that turns this off.

**Checked by eye on the mouse:** the colours in DPI colour mode, and the light changing colour in the moving LED modes.

**Not verified yet:** how the LED modes differ from each other, whether bound buttons
really work, and the window updating when DPI buttons are pressed.

Tried on Debian 13 with one mouse, USB ID `1bcf:08b8`. `lsusb` shows it as Sunplus
"HID Gaming Mouse".

## License

BSD-3-Clause. Not affiliated with Inca.
