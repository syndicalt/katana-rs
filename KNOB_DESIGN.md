# Knob style examples

These are the real-world references I'm working from. I'll put three images inline so you can actually see the options.

**1. Arc knob (like a modern plugin — Neural DSP / Soundtoys style)**
- A circular dial with a 270° arc.
- The arc is drawn as a track (dark ring) with a lit/colored portion showing current value.
- A thin indicator line/needle from center points at the value.
- Value readout is usually below the knob in small text, sometimes a floating tooltip while dragging.
- These look modern, clean, and are easy to read at a glance.
- Example: Soundtoys / Neural DSP knobs, or iOS-style DJ apps.

**2. Classic hardware-style knob (like an amp or pedal)**
- A photo-realistic 3D-ish disc with a pointer/indicator mark and a knurled edge.
- Usually skinnier profile, often with a colored stripe or dot as the pointer.
- These look like the physical hardware the Katana is emulating.
- Example: UAD plugin knobs, amp-modeler VSTs, or the Roland Boss Tone Studio's own controls.

**3. Minimal "flat" arc knob**
- Very simple: a thin circle, a colored arc, and a tiny dot at the value position.
- No needle, no 3D shading.
- Used in modern DAWs (Ableton style), web audio tools.
- Easiest to implement, looks modern but less "instrument-like".

# What I recommend

**Hybrid approach: arc knob with a pointer tip, but rendered with a subtle metallic gradient** — modern flat design + just enough depth to feel like a piece of hardware. And **vertical faders** for the EQ (like a console mixing desk), because EQ bands are traditionally sliders, not knobs.

# The component library question

Two sub-questions here:

## 1. For the knobs/controls specifically
I already wrote bespoke Knob/Fader components for the egui version. For the web version I am planning the **same approach — custom SVG components**, not a library. Why: libraries don't have good rotary knobs, and I want the exact arc/pointer/gradient behavior you'd want. Using a library would thank you with a generic slider.

That said, I'll take inspiration from (but not depend on) libraries like `react-dial-knob` or the knob patterns in `tweakpane`.

## 2. For the overall UI (layout, buttons, modals, etc.)
This is where I **should** use a component library, and I want your input. Options:

- **Radix UI / shadcn/ui** (recommended): headless primitives for accessible UI — dropdowns, dialogs, toggles, tooltips. Fully themeable with our own dark/light + theme-color system. This is the standard modern choice: you get excellent accessibility and behavior, and 100% of the visual design stays ours.
- **Mantine / Chakra**: pre-styled batteries-included libraries. Faster to start, but their visual identity fights with a custom amp-style theme, and they're heavy.
- **Everything from scratch**: most control, but I'd be re-inventing dropdown menus, tooltips, modals — all the fiddly accessibility stuff — for no benefit.

My recommendation: **Radix primitives + Tailwind for styling** (this is exactly what shadcn/ui is), plus custom SVG Knob/Fader/switch components for the instrument controls. That gives you professional, accessible base UI with a totally custom look.

# Theme system
Light + dark modes and selectable theme colors are totally doable:
- A `ThemeProvider` (CSS variables) with `light` / `dark` base modes.
- Accent color picker (a curated palette: blue, teal, orange, purple, green, red, plus custom hex).
- The knob arcs, faders, LEDs, buttons all bind to the CSS variables so everything re-skins instantly without re-render.

Before I go further — which knob style do you want?
