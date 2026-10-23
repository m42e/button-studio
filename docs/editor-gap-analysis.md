# Button Designer editing capabilities: gap analysis

**Scope:** assess the current Button Designer editor against the editing behaviors users learn from mature 2D design tools such as Inkscape, then define a practical future baseline for button artwork. This is a product planning document, not a proposal to reproduce Inkscape.

The analysis below records the baseline before implementation. See the implementation status at the end for the capabilities now available. Canvas navigation is excluded from this roadmap.

## Executive summary

Button Designer already covers the main object types and the production path: users can create and position text, preset shapes, images, QR codes, and backgrounds, organize layers, undo/redo, and export print-ready artwork. The editor is already useful for straightforward single-button designs.

The largest usability gap is the precision and composition loop around those objects. A user can drag and resize, but has limited ways to make exact adjustments, select and arrange several objects together, or align objects by snapping. Styling is also mostly flat color and opacity. These gaps become more visible as a design becomes crowded or needs to match a reference closely.

The recommended future baseline is: **easy creation, forgiving direct manipulation, precise numeric control when needed, predictable object organization, and reliable print proofing.** Keep advanced illustration capabilities optional and out of the core scope.

## Current capability snapshot

Based on the current source and README:

- **Objects:** text, six preset shapes, imported raster images, QR codes, solid/pattern/photo backgrounds.
- **Placement:** drag to move; corner handles to resize; rotation and position controls; arrow-key nudging; alignment to the artwork or one other layer.
- **Text:** editable content, installed-font search, font size, and curved text.
- **Appearance:** solid color, opacity, visibility, and image edge bleed. No separate fill and outline controls are exposed.
- **Organization:** flat layer list with visibility, ordering, duplicate, and delete. No multi-selection, groups, or lock control.
- **Workflow:** bounded undo/redo, project save/load, PNG and PDF export, CSV personalization, and MCP editing.
- **Canvas:** trim/finished-edge guide and a product preview. The editor source does not expose snapping or user guides.

## Gap matrix

| Area | Current state | User-facing gap | Priority |
| --- | --- | --- | --- |
| Selection | One selected element, selected through canvas or layer list | No additive/marquee selection; overlapping objects can be harder to target; cannot transform a composition as a unit | P0 |
| Precise transforms | Drag/resize plus position sliders, rotation slider, and some dimension fields | Numeric position is normalized without physical units; no width/height entry for all object types, scale locking, flip, or reset-to-center action | P0 |
| Alignment and spacing | Align one object to page or one reference object | No multi-object align/distribute workflow, equal spacing, center-on-button shortcut, or smart visual guides while dragging | P0 |
| Object safety and structure | Flat layer list; hide, reorder, duplicate, delete | No lock, rename, group/ungroup, or clear indication of top-to-bottom stacking while dragging | P1 |
| Text styling | Font family search, size, color, and arc curvature | No weight/style control, text alignment, line/letter spacing, outline, or reusable text style; current text is one flat object | P1 |
| Shape and image refinement | Preset shape library; image width and bleed | No editable rounded rectangle/corner radius, image crop/reposition workflow, flip, border, or shape outline controls | P1 |
| Appearance | Solid color and opacity per element/background | No separate fill/stroke, gradients, or style copy/paste; limited control for depth and separation between adjacent colors | P1 |
| Import/export interoperability | Raster image import; PNG, PDF, project exports | No SVG/vector import/export. Users moving work between common design tools may have to rasterize or rebuild it | P1 |
| Print confidence | Finished-edge guide, 300 DPI PNG, A4 PDF sheet, product preview | No explicit bleed/safe-area overlay, export preview/options, or preflight feedback for low-resolution assets and clipped content | P1 |
| Reusable work | Duplicate layers and save whole projects | No reusable components/templates, saved palettes/styles, or easy starting layouts | P2 |
| Advanced drawing | Preset shapes only | No pen/freehand, node editing, Boolean path operations, tracing, filters, or plugin system | Out of baseline scope |

Priority meaning: **P0** blocks predictable day-to-day editing as designs become more involved; **P1** materially broadens common design work and handoff; **P2** improves repeat use. “Out of baseline scope” does not mean impossible forever; it means the cost and complexity are not justified for a button-focused editor until users demonstrate the need.

## Recommended future set

### P0 — Make ordinary editing feel dependable

1. **Multi-select:** shift-click and drag-marquee selection. Show a combined bounding box; allow move, duplicate, delete, and align on the selection.
2. **Direct precision:** provide editable numeric X/Y, width/height, and rotation fields with units or clear percent labels. Add a keep-proportions toggle, center-on-artwork action, and flip horizontal/vertical for image and shape layers.
3. **Smart alignment:** snap to artwork center/edges, trim and safe-area guides, and other object centers/edges. Show temporary alignment guides and let users turn snapping off or adjust what it snaps to.
4. **Fast common actions:** add keyboard shortcuts for text, shape, image, group/ungroup when available, front/back, and duplicate; expose tooltips and a discoverable shortcut list. Support paste/duplicate and consistent undo for every edit.

### P1 — Improve composition and output quality

1. **Object organization:** rename, lock/unlock, and group/ungroup objects; keep a clear layer stack and support reordering by drag.
2. **Typography essentials:** text alignment; bold/italic or font-face choice; line-height and letter spacing; optionally outline text with a simple stroke. Keep text editable in the project.
3. **Basic appearance controls:** separate fill and outline color/width for vector-like shapes and text; offer simple linear gradients only if the UI can keep them easy to inspect and edit. Add copy/paste style.
4. **Image crop and placement:** reposition an image within its frame, choose contain/cover, and reset crop. Preserve the original embedded asset.
5. **Print preflight:** show trim, bleed, and safe area together; flag content likely to be cut and raster images below the effective export resolution. Give a final export preview with physical size and page placement.
6. **Interchange:** assess SVG import/export as the first vector interoperability step. Preserve editable native objects where possible and clearly state when imported content is flattened or unsupported.

### P2 — Support repeatable production

- Saved templates for common button layouts, including safe-area-aware text and image placeholders.
- Reusable palettes and saved styles for consistent series/brand work.
- Batch export options that mirror the visible production settings and report skipped or invalid series rows.
- Additional vector construction tools only where common button workflows support them (for example, rounded rectangles or simple icon paths).

## Make easy things easy; make hard things possible

This should guide how the future set is presented:

- **Start from intent:** prominent actions such as Add text, Add image, and Add shape; sensible defaults; select the new object and put focus in its editable field.
- **Prefer direct manipulation:** drag objects on the canvas, resize handles, inline alignment guides, and image crop handles for common work.
- **Keep precision close by:** numeric fields should be available in the selected-object panel, with units, keyboard entry, and reset/center actions. Do not make exact layout depend on dragging a slider.
- **Progressive disclosure:** show the few controls needed most often first; put detailed typography, stroke, and export settings behind clearly labeled sections.
- **Preserve a clear recovery path:** every edit is undoable; confirm destructive whole-canvas actions; never silently discard unsupported imported content.
- **Do not trade production clarity for artistic breadth:** keep trim/bleed, safe area, physical dimensions, and export quality visible while editing.

## Suggested sequencing and acceptance checks

1. **Precise editing:** numeric transforms, center/reset actions, and proportional sizing.
2. **Composition:** multi-select, group/ungroup, selection-aware align/distribute, smart guides/snapping.
3. **Appearance and typography:** text controls, fill/outline, reusable styles, image crop.
4. **Production and interoperability:** preflight, SVG assessment, templates and batch improvements.

For each release, validate with short user tasks rather than feature counts: make a centered two-line title; place and size a logo to a known physical measurement; align three icons evenly; edit an object beneath another; ensure all important content remains inside the safe area; export and verify a print sheet. The goal is reduced hesitation and fewer precision workarounds, while retaining a direct path to deeper controls.

## Reference point: Inkscape

Inkscape is useful here as a vocabulary for mature editing patterns: a selector plus multi-object transform and alignment/distribution tools; layers and grouping; snapping and guides; flexible fills/strokes; richer text spacing and alignment; and optional path editing. Those patterns informed the gaps above. Its broad illustration toolset (node editing, arbitrary Bezier paths, tracing, filters, extensions, and complex gradients) is not a requirement for Button Designer's baseline.

References:

- [Inkscape Beginners’ Guide: Align and Distribute](https://inkscape-manuals.readthedocs.io/en/latest/align-and-distribute.html)
- [Inkscape Beginners’ Guide: Grouping Objects](https://inkscape-manuals.readthedocs.io/en/latest/grouping.html)
- [Inkscape Beginners’ Guide: Styling Text](https://inkscape-manuals.readthedocs.io/en/latest/styling-text.html)
- [Inkscape Beginners’ Guide: Strokes](https://inkscape-manuals.readthedocs.io/en/latest/strokes.html)
- [Inkscape Beginners’ Guide: Managing Workspace](https://inkscape-manuals.readthedocs.io/en/latest/managing-workspace.html)

## Implementation status — 2026-10-23

| Step | Delivered |
| --- | --- |
| Precise editing | Millimeter position and dimensions, measured text bounds, numeric rotation, proportional sizing, centering, image/shape flips, and compatible version 10 project data |
| Composition | Shift-click and marquee selection, combined selection handles, bulk move/scale/rotate, alignment, center distribution, grouping/ungrouping, layer names/locks/drag ordering, and snapping with temporary guides |
| Everyday actions | Object copy/paste with embedded assets across documents, duplicate/delete, style paste, keyboard shortcuts and Help, undo/redo, and confirmation before clearing artwork |
| Appearance and typography | Independent fill and outline color/width, rounded rectangles, text alignment/line spacing/letter spacing, and the existing Font Browser for installed weights/styles |
| Images | Editable frame proportions, contain/cover, crop pan/reset, flips, preserved original embedded artwork, and consistent preview/export rendering |
| Production | Trim/bleed and a safe area inset 2 mm from the finished edge, preflight findings, an export review window, and an expandable A4 placement preview using the PDF exporter’s layout |
| Series | Entry range and copies, personalized preview, source row numbers in validation, and an explicit option to skip invalid entries with a report |
| Reuse | Three built-in layouts, saved custom templates, palettes and styles, with validation and backward-compatible project loading |
| Interchange | SVG image import at 2048 pixels on the longest side with a flattening notice; SVG export preserves supported native content and explicitly falls back to a 2400 pixel embedded image for unsupported effects |

Simple gradients remain a future option. The implemented appearance baseline uses solid fills and independent outlines. Native SVG export does not embed fonts; the review window identifies substitution risk. Projects remain the fully editable source of truth. General path drawing, tracing, Boolean operations, and extensions remain outside this baseline.

Automated coverage includes physical dimensions and units; group transforms and boundary clamps; clipboard assets and IDs; selection and keyboard focus; legacy project defaults and round trips; crop/flip/outline pixels; typography; print layout and preflight; invalid series rows; templates/styles; SVG import/export; and a smoke test of editor tools and dialogs at the minimum window size. End-to-end workflow tests combine import, styling, grouping, undo/redo, save/reopen, and personalized rendering.
