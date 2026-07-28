# Pincraft Button Studio

A native, cross-platform button designer built in Rust with `eframe` / `egui`.
It keeps the direct canvas workflow of the referenced web editor while using a
quieter desktop layout and a print-oriented export path.

## Features

- Round button sizes from 25 mm to 75 mm
- Drag-to-position text, shape, and image layers
- Align layers to the artwork page or relative to another layer
- Proportional resizing from selection corner handles
- Editable text, color, opacity, size, and visibility
- Persistent, searchable Font Browser for installed faces and styles on each text layer
- Star, heart, circle, diamond, and bolt shapes
- Solid, sunburst, confetti, grid, and cover-cropped photo backgrounds
- Layer ordering, duplication, deletion, and visibility controls
- Bounded undo and redo history
- Trim-accurate preview plus a single labeled finished-edge guide on the canvas
- PNG, JPEG, and WebP image import
- Human-readable `.buttonstudio` project save/load
- Transparent PNG export sized at 300 DPI
- CSV series printing with personalized text and QR variables

## Series printing

Open the **Series** tool and import a comma-, semicolon-, or tab-separated file whose first row contains unique column names:

```csv
name;team;url
Ada;Research;https://example.com/42
Linus;Kernel;https://example.com/7
```

Each column becomes a variable such as `{{name}}` or `{{url}}`. Select a text or QR layer and click a variable to append it to that layer, or type placeholders directly into its content. With series data loaded, **Export PDF** creates one button per CSV entry, keeps the standard A4 placement, and adds pages as needed. Remove the series data to return PDF export to its normal fill-the-page behavior.

## Run

Install the current stable Rust toolchain, then run:

```sh
cargo run --release
```

The same source builds on macOS, Windows, and Linux. The Linux build requires a
working desktop environment with WGPU support and an XDG desktop portal for
native file dialogs.

## Verify

```sh
cargo test
cargo clippy --all-targets -- -D warnings
cargo fmt --all --check
```

## Shortcuts

| Action | Shortcut |
| --- | --- |
| Undo | `Cmd/Ctrl+Z` |
| Redo | `Cmd+Shift+Z` or `Ctrl+Y` |
| Save project | `Cmd/Ctrl+S` |
| Open project | `Cmd/Ctrl+O` |
| Export PNG | `Cmd/Ctrl+E` |
| Duplicate layer | `Cmd/Ctrl+D` |
| Delete layer | `Delete` or `Backspace` |

## Files

Project files store design data as versioned JSON. Imported artwork and selected
system fonts remain linked to their original file paths, so keep those resources
available when moving a project between machines. A missing font is reported
when opening the project instead of silently changing the design.

Exports are transparent PNG files. For example, a 59 mm button exports at
697 x 697 pixels, corresponding to 300 DPI. The artwork canvas includes 5 mm
of wrap/bleed per side; the product preview and export crop that artwork to the
selected physical button diameter.

Archivo Black is bundled under the SIL Open Font License; see
[`assets/fonts/OFL.txt`](assets/fonts/OFL.txt).
