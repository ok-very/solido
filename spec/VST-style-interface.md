<img src="https://r2cdn.perplexity.ai/pplx-full-logo-primary-dark%402x.png" style="height:64px;margin-right:32px"/>

# serve this as a markdown with the VST ui "shopping list"

```md
# VST-Style UI “Shopping List” (egui + wgpu)

You’re rendering the 2.5D/SDF world via **wgpu**, and you want “classic VST skin” cell UIs as textured controls layered on top. This is very doable in egui: use wgpu for the viewport and egui for layout + input + textured widgets, plus custom GPU paint callbacks when needed. [web:429][web:442]

---

## 1) Core rendering + UI plumbing

### World viewport (wgpu-in-egui)
- **Viewport rect allocation** in `CentralPanel`, then draw your scene into that rect via a wgpu paint callback. [web:421][web:442][web:445]
- **Input routing**: pointer/keyboard focus remains in egui; you translate pointer events in the viewport rect into camera/tools. [web:419][web:416]

### UI layout containers
- `TopBottomPanel`: always-on strip (Scheme, FieldInfluence, bypass, Panic). [web:418]
- `SidePanel`: inspector (selected Link, transforms, field selection, debug toggles). [web:409]
- `CentralPanel`: world. [web:421]
- `Window` / `Area`: floating per-organism “cell UI” overlays pinned near a projected position. [web:409]

---

## 2) Visual assets to prepare (the “skin”)

### Texture assets
- **UI atlas PNG** (recommended): one big spritesheet for knobs/buttons/frames/icons to reduce texture churn. Load as an egui `TextureHandle`. [web:444][web:439]
- Optional: **per-theme atlas** (dark/bright, “bio” skins, accessibility variant).

### For each control type, you want:
- **Knob strip**: N frames (e.g., 64 or 128) for rotation states.
- **Button frames**: normal / hover / pressed / toggled (and disabled).
- **Slider frames** (if any): track + handle, plus hover/active variants.
- **Meter frames**: segmented bar or smooth fill; plus “peak hold” indicator art if you want it.
- **Port/jack icons**: Sensation / Emission / Nerves / Link, plus type badges.
- **Tiny glyph set**: scheme indicator, bypass, panic, record/replay, debug views.

---

## 3) Control widget patterns (classic VST approach)

### Pattern A: “Invisible hitbox + painted sprite”
- Use a stable interaction rect, then draw the sprite (or a UV sub-rect) on top.
- This avoids weird hit-testing when the art is funky (glows, irregular shapes). [web:430][web:419]

### Pattern B: ImageButton for clickable sprites
- Use `ImageButton` to make skinned buttons from an atlas frame; it supports selecting a UV range for sub-images. [web:443][web:449]

### Knobs (the big one)
- Display: choose frame index from parameter value, draw that atlas frame.
- Interaction: drag to change value (and optionally mouse wheel), with fine-control modifier.
- Feedback: tooltip with exact value; optional “ghost” indicator of automation/modulation.

### Sliders / faders (if you use them)
- Same concept: interaction rect + paint. Prefer consistent “direction” and sensitivity across schemes.

### Meters / readouts
- Use simple art plus egui painting for dynamic fill/needle; avoid updating textures every frame unless you really need it.

---

## 4) GPU/texture integration details (what you’ll implement)

### Texture management
- Load static UI textures once with `Context::load_texture` and store `TextureHandle`s in app state. [web:444][web:439]
- If you do dynamic textures (rare): update a `ColorImage` and push updates to the texture handle rather than constantly creating new textures. [web:424][web:427]

### Custom GPU drawing (when egui painting isn’t enough)
- Use wgpu paint callbacks for:
  - SDF viewport render.
  - Special effects that want GPU (glow, blur, distortion, “organic” UI shaders) while still letting egui own the input. [web:442][web:445]

---

## 5) “Shopping list” checklist (printable)

### Must-have (v0)
- [ ] UI atlas PNG (knob frames, button frames, icons, port badges)
- [ ] Knob widget (drag-to-turn + tooltip + fine adjust)
- [ ] Toggle button widget (skinned)
- [ ] Small meter/readout widget (for FieldInfluence, organism state)
- [ ] Patch HUD overlay: Scheme name, FieldInfluence %, bypass, Panic, live Links list
- [ ] Floating cell UI container (Window/Area pinned to organism)

### Nice-to-have (v1)
- [ ] Spritesheet UV mapping helpers (frame → UV rect)
- [ ] Theme system (swap atlas, fonts, colors)
- [ ] Animated “alive” UI accents (GPU pass or minimal texture updates)
- [ ] Parameter modulation visualization (ghost rings, delta arrows)
- [ ] Accessibility skin (higher contrast, larger labels)

### Debug instrumentation (don’t skip)
- [ ] Heatmap toggles for Fields
- [ ] “Why is this organism doing that?” inspector (top-N influences)
- [ ] Link contribution view (source → dest with effective strength after transforms)

---

## Suggested “cell UI” contents (baseline)
- 2–4 knobs: (Strength, Smoothing, Clamp, FieldInfluence local)
- 1–2 toggles: (Bypass, Invert)
- Mini meters: (local Vitality/Valence/Attention at organism position)
- Quick Link list: incoming/outgoing Links with one-click mute

---

### Next info I need (to make this concrete)
1) How many distinct “cell UI” layouts do you expect (1 shared layout, or per species/scheme)?
2) Do you want knobs to be **absolute** (jump to mouse) or **relative** (VST-style drag delta)?
```

