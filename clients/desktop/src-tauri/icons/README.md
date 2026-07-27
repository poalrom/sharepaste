# App icons

The SVGs are the source of truth; the PNG/ICO next to them are generated.

| Source              | Grid | Feeds                                                        |
| ------------------- | ---- | ------------------------------------------------------------ |
| `icon.svg`          | 1024 | `icon.png` (512²), `icon@2x.png` (1024²), `128x128@2x.png` (256²), the 64 and 256 px `icon.ico` entries |
| `icon-small.svg`    | 16   | the 16/24/32/48 px `icon.ico` entries, `tray.png` (32²), `32x32@2x.png` (64²) |
| `tray-template.svg` | 16   | `tray-template.png`                                            |

`icon.ico` is byte-identical to `../../ui/public/favicon.ico` — regenerate both together.

Every icon is a transparent glyph — no plate, no background — so it sits
directly on the dock, taskbar, menu bar or browser tab.

Two sources rather than one because the glyph is only a few pixels tall below
48 px: `icon-small.svg` re-draws it on a 16-unit grid so every edge lands on a
pixel boundary, and lifts the oldest ribbon from `#0a6b47` to `#0f8a5b`, which
is otherwise too close to a dark taskbar to separate at 3 px tall. Both sources
size the stack to ~87.5% of the canvas height so the 48 -> 64 px handoff
between them does not jump.

## Tray icons, one per platform

`tray-template.png` is a macOS template image (`app.trayIcon.iconAsTemplate`):
AppKit keeps only its alpha channel and recolors the glyph for the menu bar, so
the ribbon stack's depth is carried by opacity, not hue. It stays 16×16 —
`tray-icon` uses the pixel dimensions as menu-bar points.

`iconAsTemplate` is macOS-only (`tray-icon`'s `set_icon_as_template` compiles to
`let _ = is_template` everywhere else), so on Windows the template's black
pixels would be drawn literally and vanish into a dark taskbar.
`tauri.windows.conf.json` therefore points the tray at `tray.png` — the small
variant at 32×32, whose greens read on a light or dark taskbar and survive
Windows downscaling them to the 16/24 px tray slot.

## The macOS icns slot sizes are not free choices

`bundle.icon` lists four PNGs because `tauri-bundler` packs one icns slot per
file and errors out on any size it cannot map. `icns::IconType::from_pixel_size_and_density`
accepts a fixed table, and only some of it keeps an alpha channel:

| File               | Pixels | Density  | Slot                 |
| ------------------ | ------ | -------- | -------------------- |
| `icon.png`         | 512    | 1        | `RGBA32_512x512`     |
| `icon@2x.png`      | 1024   | 2 (@2x)  | `RGBA32_512x512_2x`  |
| `128x128@2x.png`   | 256    | 2 (@2x)  | `RGBA32_128x128_2x`  |
| `32x32@2x.png`     | 64     | 2 (@2x)  | `RGBA32_32x32_2x`    |

Density comes from the filename, not the content: `is_retina` is
`file_stem().ends_with("@2x")`. A 1024 px file without that suffix asks for
`(1024, 1024, 1)`, which is not in the table — the bundler fails with
`Failed to create app icon: No matching IconType`, as CI caught.

The 1x sizes below 256 px are deliberately absent: `(16,16,1)`, `(32,32,1)`,
`(48,48,1)` and `(128,128,1)` map to `RGB24_*` slots, which have no alpha and
would flatten these transparent glyphs onto black.

`icons/icon.ico` is deliberately NOT in `bundle.icon`. Both `tauri-codegen` and
`tauri-build` fall back to that literal path when the list holds no `.ico`, so
Windows still gets its window and resource icon, while the icns builder — which
would otherwise `image::open` the multi-size `.ico` and pack whichever frame it
picked — never sees it.

## Regenerating

With ImageMagick 7 (`brew install imagemagick`, which pulls in librsvg):

```sh
cd clients/desktop/src-tauri/icons
magick -background none icon.svg          -resize 512x512   icon.png
magick -background none icon.svg          -resize 1024x1024 icon@2x.png
magick -background none icon.svg          -resize 256x256   128x128@2x.png
magick -background none icon-small.svg    -resize 64x64     32x32@2x.png
magick -background none tray-template.svg -resize 16x16     tray-template.png
magick -background none icon-small.svg    -resize 32x32     tray.png
magick \
  \( -background none icon-small.svg -resize 48x48  \) \
  \( -background none icon-small.svg -resize 16x16  \) \
  \( -background none icon-small.svg -resize 24x24  \) \
  \( -background none icon-small.svg -resize 32x32  \) \
  \( -background none icon.svg       -resize 64x64  \) \
  \( -background none icon.svg       -resize 256x256 \) \
  icon.ico
cp icon.ico ../../ui/public/favicon.ico
```

Rasterize each entry from its own source; letting `-define icon:auto-resize`
downsample the 1024 master instead produces mush at 16 px.

**The 48 px entry must come first.** The shell picks the best-matching entry by
size, so order is irrelevant to Explorer — but `tauri-codegen` does
`&icon_dir.entries()[0]` (`tauri-codegen/src/image.rs:57`) and hands that one
bitmap to tao as the runtime window icon. tao sends it as `ICON_SMALL` only;
`set_taskbar_icon` is never called, so `ICON_BIG` stays unset and the Windows
11 taskbar scales `ICON_SMALL` to its 24 px button. With 16 px first that was a
1.5x upscale — visibly soft. 48 divides exactly by both the 24 px taskbar
button and the 16 px title bar icon, so both land on whole pixels.

## Picking the changes up

Every file here is baked into the binary at compile time — `tauri-codegen`
embeds `icon.ico` (the Windows window/taskbar icon) and the tray icon into the
generated context, and `tauri-build` writes `icon.ico` into the exe's Win32
resource table. A running `tauri dev` keeps serving the icons its binary was
compiled with, so restart it after editing anything here.

Restarting is only enough because `build.rs` lists these files explicitly.
`tauri_build::build()` declares just the config files and `capabilities/` as
build inputs; the icons are read by the `tauri::generate_context!` proc macro,
and cargo directives printed from a proc macro are discarded. Delete those
`rerun-if-changed` lines and an icon edit produces a 0.5s "Finished" with the
old artwork still compiled in. Any icon added here needs a line in `build.rs`.

If something still looks stale, force it with `cargo clean -p
sharepaste-desktop` — and note that Explorer keeps its own thumbnail cache for
the `.exe`, refreshed with `ie4uinit.exe -show`.
