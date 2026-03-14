#!/usr/bin/env python3
"""Fetch UIowa MIS samples from Wayback Machine archive.

Usage:
    # Fetch specific instruments
    python tools/samples/fetch_uiowa.py --instruments marimba,xylophone,bells

    # Fetch everything in manifest
    python tools/samples/fetch_uiowa.py --all

Downloads AIFF from Wayback Machine, converts to WAV 48kHz mono via ffmpeg,
organizes into assets/samples/uiowa/{instrument}/{instrument}.{mallet}.{dyn}.{note}.wav

Requirements:
    pip install requests
    ffmpeg must be on PATH
"""

import argparse
import json
import os
import subprocess
import sys
import tempfile
from pathlib import Path

import requests

SCRIPT_DIR = Path(__file__).parent
PROJECT_ROOT = SCRIPT_DIR.parent.parent
MANIFEST_PATH = SCRIPT_DIR / "uiowa_manifest.json"
OUTPUT_DIR = PROJECT_ROOT / "assets" / "samples" / "uiowa"

# Audio processing constants
TARGET_SR = 48000
LEAD_SILENCE_THRESHOLD_DB = -50
TRAIL_SILENCE_THRESHOLD_DB = -40
NORMALIZE_DB = -1.0
MAX_DURATION_S = 3.0


def load_manifest():
    with open(MANIFEST_PATH) as f:
        return json.load(f)


def download_file(url, dest):
    """Download a URL to a local file with progress."""
    print(f"  Downloading: {url}")
    resp = requests.get(url, stream=True, timeout=60)
    resp.raise_for_status()
    with open(dest, "wb") as f:
        for chunk in resp.iter_content(chunk_size=8192):
            f.write(chunk)
    size_mb = os.path.getsize(dest) / (1024 * 1024)
    print(f"  Downloaded: {size_mb:.1f} MB")


def convert_to_wav(input_path, output_path):
    """Convert AIFF to WAV 48kHz mono 16-bit with silence removal and normalization.

    Pipeline:
    1. Convert to 48kHz mono 16-bit WAV
    2. Remove leading silence (-50dB threshold)
    3. Remove trailing silence (-40dB threshold)
    4. Peak normalize to -1dBFS
    5. Trim to max 3s
    """
    # Single ffmpeg command with filter chain
    filters = [
        f"aresample={TARGET_SR}",
        "pan=mono|c0=0.5*c0+0.5*c1",
        # Remove leading silence
        f"silenceremove=start_periods=1:start_threshold={LEAD_SILENCE_THRESHOLD_DB}dB",
        # Remove trailing silence
        f"areverse,silenceremove=start_periods=1:start_threshold={TRAIL_SILENCE_THRESHOLD_DB}dB,areverse",
        # Normalize
        f"loudnorm=I=-16:TP={NORMALIZE_DB}:LRA=11",
        # Trim to max duration
        f"atrim=0:{MAX_DURATION_S}",
    ]

    cmd = [
        "ffmpeg", "-y",
        "-i", str(input_path),
        "-af", ",".join(filters),
        "-acodec", "pcm_s16le",
        "-ar", str(TARGET_SR),
        "-ac", "1",
        str(output_path),
    ]

    result = subprocess.run(cmd, capture_output=True, text=True)
    if result.returncode != 0:
        print(f"  ffmpeg error: {result.stderr[:500]}")
        return False
    return True


def fetch_instrument(manifest, instrument_name):
    """Fetch and process all files for one instrument."""
    base_url = manifest["base_url"]
    instrument = manifest["instruments"].get(instrument_name)
    if not instrument:
        print(f"Unknown instrument: {instrument_name}")
        return False

    out_dir = OUTPUT_DIR / instrument_name
    out_dir.mkdir(parents=True, exist_ok=True)

    for file_info in instrument["files"]:
        url = base_url + file_info["url"]
        # Derive output filename from URL
        src_filename = Path(file_info["url"]).stem  # e.g., "marimba.yarn.mf.C2B4"
        wav_filename = f"{src_filename}.wav"
        wav_path = out_dir / wav_filename

        if wav_path.exists():
            print(f"  Skip (exists): {wav_path.name}")
            continue

        with tempfile.NamedTemporaryFile(suffix=".aif", delete=False) as tmp:
            tmp_path = tmp.name

        try:
            download_file(url, tmp_path)
            print(f"  Converting: {wav_filename}")
            if convert_to_wav(tmp_path, wav_path):
                print(f"  Output: {wav_path}")
            else:
                print(f"  FAILED: {wav_filename}")
        except requests.RequestException as e:
            print(f"  Download failed: {e}")
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
            print(f"  {name} ({files} file{'s' if files > 1 else ''})")
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
