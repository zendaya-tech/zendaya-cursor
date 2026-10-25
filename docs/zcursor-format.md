# ZCursor archive format, version 1

A `.zcursor` is a UTF-8-named ZIP archive containing `theme.json`, `preview.png`,
and PNG strips under `cursors/` and `effects/`. JSON is UTF-8 without a BOM.
This document and the converter define the initial exchange format; the desktop
application does not yet load these archives.

`theme.json` identifies `format: "zcursor"` and `formatVersion: 1`, with the theme
name, author, source version and license (null when not supplied). Each cursor or
effect has a stable ID, a role, an image path, `layout: "horizontal-strip"`,
`frameSize`, `frameCount`, `frameDurationsMs`, `hotspot` and `scale`.
Frame rectangles are `(index * frameSize.width, 0, frameSize.width, frameSize.height)`.
The hotspot uses unscaled frame pixel coordinates; scale identifies the source
resolution variant. PNG strips preserve all four source channels and transparency.

## Import CursorFX files

Install Pillow in your Python environment, then run:

```powershell
python tools/convert_cursorfx.py "C:\Users\Public\Documents\Stardock\CursorFX\Themes" dist/zcursor
```

The converter reads the 184-byte container header, bounds and decompresses zlib
payloads, reads UTF-16LE metadata and walks the length-delimited image blocks.
Supported block types are cursor (2, 76/88-byte header), effect (4, 68-byte header)
and trail (8, 100-byte header), as observed in the installed themes.
Unknown layouts fail with an error rather than silently dropping content.

The original file is preserved in `source/original.cursorfx`. The JSON retains
its SHA-256, original metadata, container and block header words, state IDs,
role IDs, timing and offsets. Roles are inferred from the supplied Fluent artwork;
repeated roles, states and resolution variants remain separate entries. The raw
timing value is provisionally interpreted as milliseconds. Undocumented flags,
effect triggers and alpha-premultiplication conventions still require runtime
mapping; this is a lossless asset conversion, not a CursorFX playback engine.

Before publishing an archive, the converter reopens it and checks ZIP CRCs,
original file equality, every PNG's exact pixel round-trip to the source BGRA
buffer, animation dimensions, hotspots and the preview. A JSON conversion report
lists each output, its hash and any errors. Inputs are never changed or executed.

Converted third-party artwork belongs in ignored `dist/`, not the source repository.
Converting a theme does not change its authorship or license. The converter code
uses the repository's MIT license; that license does not apply to imported themes.
