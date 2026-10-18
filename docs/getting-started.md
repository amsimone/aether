# Getting Started with Aether

Aether is a professional rocket design and 6-DOF flight simulator. This
guide covers the Phase 1 design editor — building a rocket visually and
checking stability.

## Installation

```sh
cargo run -p rocket-ui
```

## Building Your First Rocket

1. **Start the app.** A sample rocket (nose cone + body tube) loads
   automatically.

2. **Add components** via the `Add` menu or the quick-add buttons in the
   component tree panel:
   - **Nose Cone** — the front of the rocket (balsa, tangent ogive by default)
   - **Body Tube** — the main airframe (cardboard)
   - **Fin Set** — 3 trapezoidal fins mounted on the last body tube
   - **Launch Lug** — the small tube that guides the rocket on the rod
   - **Mass Object** — ballast, altimeter, battery, etc.

3. **Edit components** by clicking them in the component tree, then
   adjusting the sliders in the Properties panel. Changes update the 3D
   view and mass/CG/CP readouts in real time.

4. **Check stability** in the Stability panel:
   - **CG** (● blue circle) — center of gravity
   - **CP** (◆ orange diamond) — center of pressure
   - **SM** — static margin in calibers (green ≥1.0 = stable, amber 0-1 =
     marginal, red <0 = unstable)

5. **Save your design** via `File → Save As…` (`.rocket` file format).

## Keyboard Shortcuts

| Shortcut | Action |
|----------|--------|
| `Ctrl+N` | New rocket |
| `Ctrl+O` | Open… |
| `Ctrl+S` | Save |
| `Ctrl+D` | Duplicate selected component |
| `Delete` | Delete selected component |

## 3D Viewport Controls

| Input | Action |
|-------|--------|
| Drag | Rotate |
| Scroll | Zoom |
| Shift+Drag | Pan |

## Tips

- A stable rocket has CP behind CG (SM ≥ 1.0 caliber).
- Add ballast in the nose to move CG forward if the rocket is unstable.
- Move fins aft to move CP backward.
- The design-time aero estimate is at M=0.1 (subsonic). The full aero
  model (Phase 3) includes compressibility, drag, and dynamic derivatives.

## Motor Data

Use the CLI to search and download motor thrust curves:

```sh
cargo run -p rocket-cli -- motors search A8
cargo run -p rocket-cli -- motors info <motor_id>
cargo run -p rocket-cli -- motors list
```

Motor data is fetched from [ThrustCurve.org](https://www.thrustcurve.org/)
and cached locally for offline use.
