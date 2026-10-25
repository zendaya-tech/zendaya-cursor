#!/usr/bin/env python3
"""Convert locally owned CursorFX themes into documented ZCursor v1 archives.

Requires Pillow. Never executes a theme or changes the source file.
"""

import argparse
import hashlib
from io import BytesIO
import json
from pathlib import Path
import struct
import zipfile
import zlib

from PIL import Image, ImageDraw


MAX_PAYLOAD = 256 * 1024 * 1024
ROLES = {
    0: "normal", 1: "help", 2: "working", 3: "busy", 4: "precision",
    5: "text", 6: "handwriting", 7: "unavailable",
    8: "resize-ns", 9: "resize-ns", 10: "resize-ew", 11: "resize-ew",
    12: "resize-nesw", 13: "resize-nesw", 14: "resize-nwse", 15: "resize-nwse",
    16: "move", 17: "alternate", 18: "link",
}


def png(image):
    stream = BytesIO()
    image.save(stream, format="PNG", optimize=True)
    return stream.getvalue()


def json_bytes(value):
    return (json.dumps(value, ensure_ascii=False, indent=2) + "\n").encode("utf-8")


def unpack_theme(data):
    if len(data) < 184:
        raise ValueError("Truncated theme header")
    header = struct.unpack_from("<46I", data)
    if header[0] != 1 or header[1] != 184:
        raise ValueError("Unsupported CursorFX container version")
    expected = header[2]
    if expected > MAX_PAYLOAD:
        raise ValueError("Uncompressed theme exceeds safety limit")
    if header[30]:
        if header[30] != len(data) - 184:
            raise ValueError("Compressed length does not match container")
        decoder = zlib.decompressobj()
        payload = decoder.decompress(data[184:], expected + 1)
        if not decoder.eof or decoder.unconsumed_tail or decoder.unused_data:
            raise ValueError("Incomplete, oversized or trailing compressed data")
    else:
        payload = data[184:]
    if len(payload) != expected:
        raise ValueError("Uncompressed length does not match container")
    # Six UTF-16LE metadata fields, addressed relative to the uncompressed payload.
    metadata = {}
    for index, key in enumerate(["name", "author", "version", "website", "email", "description"]):
        start = 0 if index == 0 else header[33 + index * 2]
        length = header[34 + index * 2]
        if start + length > len(payload) or length % 2:
            raise ValueError("Invalid metadata range")
        metadata[key] = payload[start:start + length].decode("utf-16le").rstrip("\0")
    return header, payload, metadata


def convert(source, destination):
    original = source.read_bytes()
    header, payload, metadata = unpack_theme(original)
    manifest = {
        "format": "zcursor", "formatVersion": 1,
        "name": metadata["name"] or source.stem,
        "author": metadata["author"] or None,
        "version": metadata["version"] or None,
        "license": None,
        "description": metadata["description"] or "",
        "preview": "preview.png",
        "cursors": [], "effects": [],
        "source": {
            "format": "CursorFX", "filename": source.name,
            "sha256": hashlib.sha256(original).hexdigest(),
            "original": "source/original.cursorfx", "metadata": metadata,
            "headerWords": list(header),
        },
        "conversionNotes": [
            "Role names are inferred from the supplied Fluent artwork; source role IDs remain authoritative.",
            "Timing values are interpreted as milliseconds; CursorFX timing and behavior flags are retained verbatim.",
            "Effect triggers, state IDs and undocumented flags require runtime mapping; they are not discarded.",
            "PNG channels preserve the source BGRA bytes as RGBA without changing alpha or premultiplication.",
            "Original theme artwork and its license remain the original author's property.",
        ],
    }
    assets = {"source/original.cursorfx": original}
    previews = []
    offset = header[45]
    index = 0
    while offset < len(payload):
        if offset + 12 > len(payload):
            raise ValueError("Truncated block")
        kind, header_size, size = struct.unpack_from("<III", payload, offset)
        if header_size % 4 or size < header_size or offset + size > len(payload):
            raise ValueError("Invalid block size")
        if (kind, header_size) not in {(2, 76), (2, 88), (4, 68), (8, 100)}:
            raise ValueError(f"Unsupported block {kind}/{header_size}")
        words = struct.unpack_from("<" + "I" * (header_size // 4), payload, offset)
        if kind == 2:
            role_id, state, frames, width, height, timing = words[4], words[5], *words[7:11]
            hotspot = list(words[13:15])
            image_offset, image_size = words[17], words[16]
            scale = struct.unpack_from("<d", payload, offset + 80)[0] if header_size == 88 else 1.0
            role = ROLES.get(role_id, f"cursorfx-{role_id}")
            category = "cursors"
        else:
            frames, width, height, timing = words[5:9]
            hotspot = list(words[11:13])
            image_offset, image_size = words[15], words[14]
            role_id, state, scale = None, None, 1.0
            role = "cursorfx-effect" if kind == 4 else "cursorfx-trail"
            category = "effects"
        if not frames or not width or not height or width % frames:
            raise ValueError("Invalid horizontal frame strip")
        if image_size != width * height * 4 or image_offset < header_size or image_offset + image_size > size:
            raise ValueError("Invalid pixel buffer")
        pixels = payload[offset + image_offset:offset + image_offset + image_size]
        image = Image.frombytes("RGBA", (width, height), pixels, "raw", "BGRA")
        image_path = f"{category}/{index:03d}-{role}.png"
        assets[image_path] = png(image)
        frame_width = width // frames
        manifest[category].append({
            "id": f"cursorfx-block-{index}", "role": role,
            "image": image_path, "layout": "horizontal-strip",
            "frameSize": {"width": frame_width, "height": height},
            "frameCount": frames, "frameDurationsMs": [timing] * frames,
            "hotspot": {"x": hotspot[0], "y": hotspot[1]}, "scale": scale,
            "source": {"blockType": kind, "roleId": role_id, "stateId": state,
                       "timingRaw": timing, "payloadOffset": offset, "headerWords": list(words)},
        })
        if len(previews) < 40:
            previews.append((f"{index}: {role}", image.crop((0, 0, frame_width, height))))
        offset += size
        index += 1
    if not index:
        raise ValueError("Theme has no image blocks")
    preview = Image.new("RGB", (720, ((len(previews) + 5) // 6) * 116 + 40), "#29272f")
    draw = ImageDraw.Draw(preview)
    draw.text((12, 12), source.stem, fill="white")
    for i, (label, image) in enumerate(previews):
        image.thumbnail((72, 72))
        x, y = i % 6 * 120, i // 6 * 116 + 40
        preview.paste(image, (x + (120 - image.width) // 2, y), image)
        draw.text((x + 4, y + 78), label, fill="white")
    assets["preview.png"] = png(preview)
    assets["theme.json"] = json_bytes(manifest)
    target = destination / (source.stem + ".zcursor")
    temporary = target.with_suffix(".zcursor.tmp")
    try:
        with zipfile.ZipFile(temporary, "w", zipfile.ZIP_DEFLATED, compresslevel=9) as archive:
            for name, data in assets.items():
                archive.writestr(name, data)
        verify(temporary, original, payload)
        temporary.replace(target)
    finally:
        temporary.unlink(missing_ok=True)
    return {"file": target.name, "cursors": len(manifest["cursors"]),
            "effects": len(manifest["effects"]), "bytes": target.stat().st_size,
            "sha256": hashlib.sha256(target.read_bytes()).hexdigest()}


def verify(target, original, payload):
    """Round-trip every image, frame size, hotspot and source header from the ZIP."""
    with zipfile.ZipFile(target) as archive:
        if archive.testzip() is not None:
            raise ValueError("Archive CRC failure")
        manifest = json.loads(archive.read("theme.json"))
        if archive.read(manifest["source"]["original"]) != original:
            raise ValueError("Original source was not preserved")
        for entry in manifest["cursors"] + manifest["effects"]:
            source = entry["source"]
            words = source["headerWords"]
            offset = source["payloadOffset"]
            if source["blockType"] == 2:
                image_offset, image_size = words[17], words[16]
                hotspot = words[13:15]
            else:
                image_offset, image_size = words[15], words[14]
                hotspot = words[11:13]
            with Image.open(BytesIO(archive.read(entry["image"]))) as image:
                if image.size != (entry["frameSize"]["width"] * entry["frameCount"], entry["frameSize"]["height"]):
                    raise ValueError("Frame dimensions changed")
                if image.tobytes("raw", "BGRA") != payload[offset + image_offset:offset + image_offset + image_size]:
                    raise ValueError("Image pixels changed")
            if [entry["hotspot"]["x"], entry["hotspot"]["y"]] != hotspot:
                raise ValueError("Hotspot changed")
        with Image.open(BytesIO(archive.read("preview.png"))) as image:
            image.verify()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("source", type=Path)
    parser.add_argument("destination", type=Path)
    args = parser.parse_args()
    args.destination.mkdir(parents=True, exist_ok=True)
    files = [args.source] if args.source.is_file() else sorted(
        p for p in args.source.iterdir() if p.suffix.lower() == ".cursorfx")
    results, failures = [], []
    for source in files:
        try:
            result = convert(source, args.destination)
            results.append(result)
            print(f"OK {source.name}: {result['cursors']} cursors, {result['effects']} effects", flush=True)
        except Exception as error:
            failures.append({"file": source.name, "error": str(error)})
            print(f"ERROR {source.name}: {error}", flush=True)
    report = {"format": "zcursor", "formatVersion": 1, "converted": results, "failures": failures}
    (args.destination / "conversion-report.json").write_bytes(json_bytes(report))
    print(f"Converted {len(results)}/{len(files)} themes; {len(failures)} failures")
    if failures or not files:
        raise SystemExit(1)


if __name__ == "__main__":
    main()
