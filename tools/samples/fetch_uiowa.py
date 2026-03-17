#!/usr/bin/env python3
"""Fetch UIowa MIS samples from Wayback Machine archive.

Usage:
    # Fetch specific instruments
    python tools/samples/fetch_uiowa.py --instruments marimba,xylophone,bells

    # Fetch everything in manifest
    python tools/samples/fetch_uiowa.py --all

Downloads range-file AIFFs from Wayback Machine, splits into individual notes
by silence detection, converts to WAV 48kHz mono 16-bit. Applies peak
normalization and silence trimming.

Organizes output into assets/samples/uiowa/{instrument}/{filename}.wav

Requirements:
    pip install requests
"""

import argparse
import json
import os
import struct
import sys
import tempfile
import wave
from pathlib import Path

import requests

SCRIPT_DIR = Path(__file__).parent
PROJECT_ROOT = SCRIPT_DIR.parent.parent
MANIFEST_PATH = SCRIPT_DIR / "uiowa_manifest.json"
OUTPUT_DIR = PROJECT_ROOT / "assets" / "samples" / "uiowa"

# Audio processing constants
TARGET_SR = 48000
SILENCE_THRESHOLD = 0.005  # Linear amplitude threshold for silence detection
MIN_GAP_SAMPLES = 4000     # Minimum silence gap between notes (~83ms at 48kHz)
MIN_NOTE_SAMPLES = 8000    # Minimum note length (~167ms at 48kHz)
MAX_SAMPLES = TARGET_SR * 4  # 4 seconds max per note (default; overridden by per-instrument max_duration_sec)

# Chromatic note names (sharps)
NOTE_NAMES = ['C', 'Cs', 'D', 'Ds', 'E', 'F', 'Fs', 'G', 'Gs', 'A', 'As', 'B']


def load_manifest():
    with open(MANIFEST_PATH) as f:
        return json.load(f)


def parse_note(note_str):
    """Parse note string like 'C4', 'Cs5', 'F4' into (note_index, octave)."""
    note_str = note_str.strip()
    if len(note_str) >= 3 and note_str[1] in ('s', '#'):
        name = note_str[0].upper() + 's'
        octave = int(note_str[2:])
    elif len(note_str) >= 2 and note_str[1] in ('b',):
        # Convert flat to sharp equivalent
        flat_name = note_str[0].upper()
        flat_idx = NOTE_NAMES.index(flat_name)
        name = NOTE_NAMES[(flat_idx - 1) % 12]
        octave = int(note_str[2:])
    else:
        name = note_str[0].upper()
        octave = int(note_str[1:])
    idx = NOTE_NAMES.index(name)
    return idx, octave


def note_name_at(start_note_str, offset):
    """Get the note name at `offset` semitones above `start_note_str`."""
    idx, octave = parse_note(start_note_str)
    total = idx + offset
    new_idx = total % 12
    new_octave = octave + total // 12
    return f"{NOTE_NAMES[new_idx]}{new_octave}"


def download_file(url, dest):
    """Download a URL to a local file."""
    print(f"  Downloading: {url}")
    resp = requests.get(url, stream=True, timeout=120)
    resp.raise_for_status()
    with open(dest, "wb") as f:
        for chunk in resp.iter_content(chunk_size=8192):
            f.write(chunk)
    size_mb = os.path.getsize(dest) / (1024 * 1024)
    print(f"  Downloaded: {size_mb:.1f} MB")


def _parse_aiff_extended(data):
    """Parse 80-bit IEEE 754 extended precision float (AIFF sample rate encoding)."""
    exponent = ((data[0] & 0x7F) << 8) | data[1]
    mantissa = 0
    for i in range(2, 10):
        mantissa = (mantissa << 8) | data[i]
    sign = -1 if data[0] & 0x80 else 1
    if exponent == 0 and mantissa == 0:
        return 0.0
    exponent -= 16383
    return sign * (mantissa / (1 << 63)) * (2 ** exponent)


def read_aiff(path):
    """Read an AIFF/AIFF-C file and return (samples_float, sample_rate, channels).

    Pure-bytes parser -- no aifc module needed (removed in Python 3.13).
    Returns samples as list of float [-1, 1].
    """
    with open(str(path), "rb") as f:
        data = f.read()

    if data[:4] != b"FORM":
        raise ValueError("Not an AIFF file (missing FORM header)")
    form_type = data[8:12]
    if form_type not in (b"AIFF", b"AIFC"):
        raise ValueError(f"Not AIFF: form type = {form_type}")

    n_channels = 0
    sampwidth = 0
    sr = 44100
    n_frames = 0
    sound_data = b""

    pos = 12
    while pos < len(data) - 8:
        chunk_id = data[pos:pos+4]
        chunk_size = struct.unpack(">I", data[pos+4:pos+8])[0]
        chunk_data = data[pos+8:pos+8+chunk_size]

        if chunk_id == b"COMM":
            n_channels = struct.unpack(">h", chunk_data[0:2])[0]
            n_frames = struct.unpack(">I", chunk_data[2:6])[0]
            sampwidth = struct.unpack(">h", chunk_data[6:8])[0] // 8
            sr = int(_parse_aiff_extended(chunk_data[8:18]))

        elif chunk_id == b"SSND":
            offset = struct.unpack(">I", chunk_data[0:4])[0]
            sound_data = chunk_data[8 + offset:]

        pos += 8 + chunk_size
        if chunk_size % 2 == 1:
            pos += 1

    if not sound_data:
        raise ValueError("No SSND chunk found in AIFF")

    total_samples = n_frames * n_channels
    raw = sound_data[:total_samples * sampwidth]

    if sampwidth == 2:
        fmt = f">{total_samples}h"
        int_samples = struct.unpack(fmt, raw)
        max_val = 32768.0
    elif sampwidth == 3:
        int_samples = []
        for i in range(0, len(raw), 3):
            b = raw[i:i+3]
            val = int.from_bytes(b, byteorder="big", signed=True)
            int_samples.append(val)
        max_val = 8388608.0
    elif sampwidth == 1:
        fmt = f">{total_samples}b"
        int_samples = struct.unpack(fmt, raw)
        max_val = 128.0
    else:
        raise ValueError(f"Unsupported sample width: {sampwidth} bytes")

    float_samples = [s / max_val for s in int_samples]
    return float_samples, sr, n_channels


def downmix_mono(samples, channels):
    """Downmix multi-channel to mono by averaging."""
    if channels == 1:
        return samples
    mono = []
    for i in range(0, len(samples), channels):
        frame = samples[i:i+channels]
        mono.append(sum(frame) / len(frame))
    return mono


def resample_linear(samples, src_sr, dst_sr):
    """Simple linear interpolation resampling."""
    if src_sr == dst_sr:
        return samples
    ratio = src_sr / dst_sr
    n_out = int(len(samples) / ratio)
    out = []
    for i in range(n_out):
        src_pos = i * ratio
        idx = int(src_pos)
        frac = src_pos - idx
        if idx + 1 < len(samples):
            val = samples[idx] * (1 - frac) + samples[idx + 1] * frac
        elif idx < len(samples):
            val = samples[idx]
        else:
            break
        out.append(val)
    return out


def split_notes(samples, threshold=SILENCE_THRESHOLD, min_gap=MIN_GAP_SAMPLES, min_note=MIN_NOTE_SAMPLES):
    """Split a range file into individual notes by detecting silence gaps.

    Returns a list of sample buffers, one per note, in order.
    """
    notes = []
    in_note = False
    note_start = 0
    silence_count = 0

    for i, s in enumerate(samples):
        if abs(s) > threshold:
            if not in_note:
                in_note = True
                note_start = max(0, i - 200)  # Keep 200 samples before onset
                silence_count = 0
            else:
                silence_count = 0
        else:
            if in_note:
                silence_count += 1
                if silence_count >= min_gap:
                    # End of note
                    note_end = i - silence_count + 200  # Keep 200 samples of tail
                    note_samples = samples[note_start:note_end]
                    if len(note_samples) >= min_note:
                        notes.append(note_samples)
                    in_note = False
                    silence_count = 0

    # Capture final note if still in progress
    if in_note:
        note_samples = samples[note_start:]
        if len(note_samples) >= min_note:
            notes.append(note_samples)

    return notes


def trim_silence(samples, threshold=SILENCE_THRESHOLD):
    """Remove leading and trailing silence."""
    start = 0
    for i, s in enumerate(samples):
        if abs(s) > threshold:
            start = max(0, i - 100)
            break
    else:
        return samples

    end = len(samples)
    for i in range(len(samples) - 1, -1, -1):
        if abs(samples[i]) > threshold:
            end = min(len(samples), i + 100)
            break

    return samples[start:end]


def peak_normalize(samples, target_db=-1.0):
    """Peak normalize to target dBFS."""
    peak = max(abs(s) for s in samples) if samples else 0.0
    if peak < 1e-6:
        return samples
    target_linear = 10 ** (target_db / 20.0)
    gain = target_linear / peak
    return [s * gain for s in samples]


def write_wav(path, samples, sr):
    """Write mono float samples as 16-bit WAV."""
    with wave.open(str(path), "wb") as f:
        f.setnchannels(1)
        f.setsampwidth(2)
        f.setframerate(sr)
        int_samples = []
        for s in samples:
            clamped = max(-1.0, min(1.0, s))
            int_samples.append(int(clamped * 32767))
        data = struct.pack(f"<{len(int_samples)}h", *int_samples)
        f.writeframes(data)


def process_range_file(input_path, output_dir, instrument, mallet, dynamic, start_note, max_duration_sec=None):
    """Process a range AIFF file: read, split notes, process each, save as individual WAVs."""
    max_samples = int(TARGET_SR * max_duration_sec) if max_duration_sec else MAX_SAMPLES

    try:
        samples, sr, channels = read_aiff(input_path)
    except Exception as e:
        print(f"  Read error: {e}")
        return []

    print(f"  Source: {sr}Hz, {channels}ch, {len(samples)//channels} frames")

    # Downmix, resample, then normalize before splitting
    mono = downmix_mono(samples, channels)
    resampled = resample_linear(mono, sr, TARGET_SR)

    # Normalize to near-unity so split threshold works regardless of source level
    pre_peak = max(abs(s) for s in resampled) if resampled else 0.0
    if pre_peak > 1e-6:
        norm_gain = 0.9 / pre_peak
        resampled = [s * norm_gain for s in resampled]
    print(f"  Resampled: {len(resampled)} samples ({len(resampled)/TARGET_SR:.1f}s), pre-norm peak: {pre_peak:.4f}")

    # Split into individual notes
    notes = split_notes(resampled)
    print(f"  Split into {len(notes)} notes (starting from {start_note})")

    saved = []
    for i, note_samples in enumerate(notes):
        note_name = note_name_at(start_note, i)

        # Process individual note
        trimmed = trim_silence(note_samples)
        if len(trimmed) > max_samples:
            trimmed = trimmed[:max_samples]
        normalized = peak_normalize(trimmed)

        # Build output filename matching SampleRegistry convention:
        # {instrument}.{mallet}.{dynamic}.{Note}.wav
        wav_filename = f"{instrument}.{mallet}.{dynamic}.{note_name}.wav"
        wav_path = output_dir / wav_filename

        duration = len(normalized) / TARGET_SR
        print(f"    {note_name}: {len(normalized)} samples ({duration:.2f}s) -> {wav_filename}")

        write_wav(wav_path, normalized, TARGET_SR)
        saved.append(wav_path)

    return saved


def fetch_instrument(manifest, instrument_name):
    """Fetch and process all range files for one instrument."""
    base_url = manifest["base_url"]
    wayback = manifest.get("wayback_prefix", "")
    instrument = manifest["instruments"].get(instrument_name)
    if not instrument:
        print(f"Unknown instrument: {instrument_name}")
        return False

    out_dir = OUTPUT_DIR / instrument_name
    out_dir.mkdir(parents=True, exist_ok=True)

    max_duration_sec = instrument.get("max_duration_sec")

    for file_info in instrument["files"]:
        start_note = file_info["start_note"]
        mallet = file_info["mallet"]
        dynamic = file_info["dynamic"]

        # Build URL: try Wayback first (direct site requires auth)
        relative_url = file_info["url"]
        direct_url = base_url + relative_url
        wayback_url = f"{wayback}/{direct_url}" if wayback else ""

        src_name = Path(relative_url).stem
        print(f"\n  --- {src_name} (notes from {start_note}) ---")

        # Check if we already have any notes from this range
        expected_first = f"{instrument_name}.{mallet}.{dynamic}.{start_note}.wav"
        if (out_dir / expected_first).exists():
            print(f"  Skip (exists): {expected_first}")
            continue

        with tempfile.NamedTemporaryFile(suffix=".aif", delete=False) as tmp:
            tmp_path = tmp.name

        try:
            downloaded = False
            # Try Wayback first (direct site requires auth now)
            for url in [wayback_url, direct_url]:
                if not url:
                    continue
                try:
                    download_file(url, tmp_path)
                    downloaded = True
                    break
                except requests.RequestException as e:
                    print(f"  Failed: {e}")
                    continue

            if not downloaded:
                print(f"  SKIP: could not download {src_name}")
                continue

            saved = process_range_file(tmp_path, out_dir, instrument_name, mallet, dynamic, start_note, max_duration_sec)
            print(f"  Saved {len(saved)} note files")

        except Exception as e:
            print(f"  Error: {e}")
        finally:
            if os.path.exists(tmp_path):
                os.unlink(tmp_path)

    return True


def main():
    parser = argparse.ArgumentParser(description="Fetch UIowa MIS samples from Wayback Machine")
    parser.add_argument("--instruments", type=str, help="Comma-separated instrument names")
    parser.add_argument("--all", action="store_true", help="Fetch all instruments in manifest")
    parser.add_argument("--list", action="store_true", help="List available instruments")
    args = parser.parse_args()

    manifest = load_manifest()

    if args.list:
        print("Available instruments:")
        for name, info in manifest["instruments"].items():
            files = len(info["files"])
            print(f"  {name} ({files} range file{'s' if files > 1 else ''})")
        return

    if not args.instruments and not args.all:
        parser.print_help()
        return

    instruments = (
        list(manifest["instruments"].keys()) if args.all
        else [s.strip() for s in args.instruments.split(",")]
    )

    OUTPUT_DIR.mkdir(parents=True, exist_ok=True)
    print(f"Output directory: {OUTPUT_DIR}")

    for name in instruments:
        print(f"\n=== {name} ===")
        fetch_instrument(manifest, name)

    print("\nDone.")


if __name__ == "__main__":
    main()
