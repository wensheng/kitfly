# kitfly

**Fly a tiny 3D plane through procedural terrain without leaving your terminal.**

(kitfly runs on terminals that support the Kitty graphics protocol:
[**Ghostty**](https://ghostty.org/),
[**Kitty**](https://sw.kovidgoyal.net/kitty/),
[**WezTerm**](https://wezterm.net/),
[**cmux**](https://github.com/manaflow-ai/cmux))

## Install

    cargo install kitfly

---

## Why

- Stop opening a game window just to test a real-time 3D scene, camera, or controls loop.
- Turn Kitty graphics into an actual flyable viewport, with Bevy rendering behind the scenes.
- Keep the whole experience terminal-native: alternate screen, raw keyboard input, resize-aware frames, and no window handoff.

---

<!-- ## Show, Don't Tell

![kitfly demo placeholder](./assets/demo.gif)
-->

## Key Capabilities

- **Fly a Bevy 3D scene directly in your terminal** with compressed Kitty graphics frames and a follow camera.
- **Cruise over procedural block terrain** with grass, trees, mountains, clouds, and chunks that stream around the plane.
- **Cycle through bundled plane models** from `assets/planes.cfg`, including model-specific scale, orientation, and propeller animation settings.

---

## Usage

```bash
kitfly
kitfly --fps 60
kitfly --resolution-scale 0.75
kitfly --fps 24 --resolution-scale 0.5
```

---

## How It Works

```text
Bevy 3D scene -> offscreen RGBA target -> GPU readback
        -> zlib + base64 Kitty frames -> terminal viewport
        -> crossterm keyboard input -> flight state + follow camera
```

kitfly runs Bevy without a primary window, renders into an offscreen image sized from the terminal's pixel dimensions, reads each frame back from the GPU, and streams the latest frame as Kitty graphics. The terminal loop owns raw mode, resize handling, frame pacing, and the status row while Bevy owns the scene, terrain, camera, plane models, and animation.

---

## Controls

| Action | Control |
| --- | --- |
| Pitch up / down | Up / Down arrows |
| Turn left / right | Left / Right arrows |
| Roll left / right | `a` / `d` |
| Increase / decrease speed | `w` / `x` |
| Cycle plane model | `s` |
| Reset flight state | `Space` |
| Quit | `q`, `Esc`, or `Ctrl-C` |
