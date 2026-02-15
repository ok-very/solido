# LCARS Connector Visual References

Gathered from GitHub LCARS projects and canonical sources. Context: our current Bezier spline connectors look wrong — thick organic curves dominating the viewport, crossing each other, no visual relationship to the LCARS language.

---

## 1. Key Insight: Canonical LCARS Doesn't Use Freeform Splines

LCARS visual language uses:
- **Elbows** (L-shaped corners with specific radii)
- **Rails/bars** (consistent-width horizontal/vertical elements)
- **End caps** (rounded terminations)
- **Color-coded sections** (hierarchy through color, not connector lines)

Connections between panels are implied through:
- Shared color coding
- Elbow-connected rails framing content areas
- Text labels within bars
- Spatial adjacency

---

## 2. Canonical LCARS Dimensions (from ha-lcars, 495 stars)

```
--lcars-vertical-border:  35px
--lcars-horizontal-border: 10px
--lcars-outer-radius:     34px
--lcars-inner-radius:     20px
--lcars-font:             Tungsten (fallback: Antonio)
--header-font-size:       40px
```

## 3. LCARS Elbow Formula (from LCARS-SDK)

- **Outer radius** = `(elbow_width + inner_radius) / 2`
- **Inner radius**: 35px default
- **Core rule**: "the midpoint of the arc is constant" — inner and outer curves share harmonious geometry
- CSS border-radius tied to element dimensions, not static values
- Named size classes map to specific `arc-X-X` configurations

## 4. LCARS Color Palette (from thelcars.com — Classic Theme)

```
african-violet: #cc99ff    almond:         #ffaa90
almond-creme:   #ffbbaa    blue:           #5566ff
bluey:          #8899ff    butterscotch:   #ff9966
gold:           #ffaa00    golden-orange:  #ff9900
gray:           #666688    green:          #999933
ice:            #99ccff    lilac:          #cc55ff
lima-bean:      #cccc66    magenta:        #cc5599
mars:           #ff2200    moonlit-violet: #9966ff
orange:         #ff8800    peach:          #ff8866
red:            #cc4444    sky:            #aaaaff
space-white:    #f5f6fa    sunflower:      #ffcc99
tomato:         #ff5555    violet-creme:   #ddbbff
```

Nemesis Blue additions: cool #6699ff, evening #2266ff, ghost #88bbff, midnight #2233ff

## 5. ha-lcars Structural Patterns

- **UI hierarchy via color variables**: `--lcars-ui-primary` (almond-creme), `--lcars-ui-secondary` (african-violet), `--lcars-ui-tertiary` (red), `--lcars-ui-quaternary` (gray)
- **Card color by position**: top=bluey, mid-left=red, button=red, bottom=gray
- Vertical stacks with header/middle/footer class styling
- Bar elements with integrated icons and state indicators

## 6. cb-lcars Component Architecture (101 stars)

- **Elbow-based framing** as primary visual connectors (not lines)
- No freeform line elements — continuity through strategic placement of header/footer elbow pairs
- **Double-elbow** (Picard variants) for additional composition
- Shape language for buttons: lozenges, bullets, capped, Picard variants
- Symbiont mode: encapsulate cards and imprint LCARS border styling
- "Contained," "open," and "callout" elbow variants for different border extensions

## 7. LCARS Proportional Grid (from joernweissenborn.github.io/lcars)

- Unit-based sizing: `lcars-u-X` (0-16 range)
- Bars: height = 1/3 of unit size
- End caps: width and height = 1/3 of unit height, rounded
- Spacer variants: single, double, left-space, right-double-space
- Flexible proportional design over fixed pixel values

---

## 8. What's Wrong With Our Current Approach

From the visual test at 1920x1080 viewport:

1. **60px thick Bezier curves** dominating the viewport (2.5u at unit_px=24)
2. **Organic curves crossing** in the center gap — spaghetti, not LCARS
3. **No visual relationship** to the LCARS language of elbows and bars
4. **Connectors overwhelm** the UI elements they connect
5. **Bus rendered as invisible backbone** — in LCARS, the bus should BE a visible rail element
6. **Purple bus-branch** looks like a giant progress bar, not a routed connection

## 9. Questions for Architect

1. Are freeform Bezier connectors the right metaphor at all? Should connections use rectilinear elbow routing instead?
2. Should the bus be a visible LCARS rail (drawn bar with end caps) rather than an invisible routing backbone?
3. Should "connection" be expressed through color and spatial adjacency rather than explicit lines?
4. If we keep line connectors, should they be:
   - Much thinner (schematic traces, not thick ribbons)?
   - Rectilinear (Manhattan routing with rounded elbows)?
   - A hybrid: elbows for bus segments, thin arcs for direct connections?
5. How should connector visual weight relate to panel visual weight?

## 10. Source Projects

- [ha-lcars](https://github.com/th3jesta/ha-lcars) — 495 stars, HA theme, CSS
- [cb-lcars](https://github.com/snootched/cb-lcars) — 101 stars, component library
- [lcarsde](https://github.com/lcarsde/lcarsde) — 129 stars, desktop environment
- [LCARS-SDK elbow discussion](https://github.com/Aricwithana/LCARS-SDK/issues/4) — radius formula
- [LCARS design grid](https://joernweissenborn.github.io/lcars/) — proportional system
- [thelcars.com/colors](https://www.thelcars.com/colors.php) — canonical palette
