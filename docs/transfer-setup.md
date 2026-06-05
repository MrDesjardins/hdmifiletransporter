# HDMI File Transfer — Setup Instructions

This guide walks through setting up a file transfer from a **transmitter** (source
computer) to a **receiver** (destination computer) using an HDMI display link and a
USB video capture card.

The transmitter embeds your file into a video and plays it on a monitor. The
receiver records that HDMI signal through a capture card and decodes the file back
to bytes. For background on the encoding format and resilience trade-offs, see
[findings.md](findings.md).

---

## What you need

| Role | Computer | Hardware | Software |
|------|----------|----------|----------|
| **Transmitter** | Source machine (often restricted — no USB, no network) | HDMI output (GPU or port) | `hdmifiletransporter`, a video player |
| **Receiver** | Destination machine (unrestricted) | USB HDMI capture card | `hdmifiletransporter`, `ffmpeg`, OpenCV dev libs |
| **Link** | — | HDMI cable: transmitter HDMI out → capture card HDMI in | — |

```mermaid
flowchart LR
  file[File to send] --> inject[Inject into MKV]
  inject --> play[Loop on HDMI display]
  play --> hdmi[HDMI cable]
  hdmi --> cap[USB capture card]
  cap --> record[Record with ffmpeg]
  record --> extract[Extract with CLI]
  extract --> out[Recovered file]
```

---

## Before you start: pick one shared configuration

Inject and extract **must use identical settings**. Write them down and use the
same values on both computers.

| Setting | Flag | Notes |
|---------|------|-------|
| Resolution | `--width`, `--height` | Must match what the capture card records (commonly `1920` × `1080`) |
| Frame rate | `--fps` | Must match playback and capture rate (e.g. `30`; typical for USB capture cards) |
| Cell size | `--size` | Pixels per encoded symbol; must divide width and height evenly |
| Algorithm | `--algo` | See recommendations below |
| Levels | `--levels` | Only for `quantized` and `brightness`; must be a power of two (`2`–`256`) |

### Recommended presets (real HDMI capture)

| Goal | Command flags | When to use |
|------|---------------|-------------|
| **Fastest (decent link)** | `--algo quantized --levels 2 --size 6` | Good cable and capture card; ~3 bits per cell |
| **Robust (flaky link)** | `--algo brightness --levels 8 --size 8` | More margin against distortion |
| **Conservative (worst link)** | `--algo brightness --levels 4 --size 6` | Survives harsh capture conditions |
| **Maximum reliability** | `--algo bw --size 6` | Slowest but most tolerant |

Avoid `--algo rgb` over a real HDMI path; colour compression on capture cards
usually corrupts dense RGB encoding.

**Example shared config** used in the steps below:

```
--width 1920 --height 1080 --fps 30 --algo quantized --levels 2 --size 6
```

---

## Part 1 — Transmitter (source computer)

The transmitter prepares the payload video and plays it in a loop on the HDMI
display.

### Step 1: Install the toolchain

Install Rust if it is not already available:

```sh
rustup toolchain install stable
rustup default stable
```

### Step 2: Install `hdmifiletransporter`

From [crates.io](https://crates.io/crates/hdmifiletransporter):

```sh
cargo install hdmifiletransporter
```

Or build from this repository:

```sh
git clone https://github.com/MrDesjardins/hdmifiletransporter.git
cd hdmifiletransporter
cargo build --release
# binary: target/release/hdmifiletransporter
```

### Step 3: Encode the file into a video

Replace `myfile.zip` with the file you want to send. The output is a **lossless**
`.mkv` (FFV1 codec). Do not use lossy containers such as `.mp4` for the encoded
source video.

```sh
hdmifiletransporter \
  -m inject \
  -i myfile.zip \
  -o transfer.mkv \
  --width 1920 --height 1080 \
  --fps 30 \
  --algo quantized --levels 2 \
  --size 6 \
  -p true
```

Copy `transfer.mkv` to the transmitter if you built it elsewhere (USB is often
blocked on the source machine — plan how the file reaches the transmitter
beforehand).

### Step 4: Connect the HDMI display

1. Plug an HDMI cable from the transmitter's **HDMI output** into the capture
   card on the receiver (or into a monitor that feeds the capture card, depending
   on your wiring).
2. Set the display to the **same resolution** as your encoding (`1920×1080` in
   the example above). Mismatched resolution causes registration failures on the
   receiver.

### Step 5: Play the video in a loop

Open `transfer.mkv` in any video player on the transmitter and **enable loop /
repeat**. The video must cycle continuously until the receiver has captured enough
clean frames.

Tips:

- Use **fullscreen** so the entire frame is visible.
- Disable overlays, notifications, and power-saving that dim or pause playback.
- The first frame of each loop is a **red** start frame (visual cue only); you
  should see it flash on every pass. The receiver needs several full loops to
  recover every data page (CRC rejects bad frames and retries on the next loop).

Example with `mpv`:

```sh
mpv --loop=inf --fullscreen transfer.mkv
```

Example with `vlc`:

```sh
vlc --loop transfer.mkv
```

Leave playback running while the receiver captures (Part 2).

---

## Part 2 — Receiver (destination computer)

The receiver records the HDMI signal, then decodes the captured video back to the
original file.

### Step 1: Install dependencies

**Rust:**

```sh
rustup toolchain install stable
rustup default stable
```

**OpenCV** (required for frame registration during extract):

```sh
# Debian / Ubuntu / WSL
sudo apt update
sudo apt install libopencv-dev clang libclang-dev cmake ffmpeg
```

**`hdmifiletransporter`:**

```sh
cargo install hdmifiletransporter
```

Or build from the repository (see Part 1, Step 2).

### Step 2: Connect and identify the capture card

1. Plug the capture card into USB.
2. Connect HDMI from the transmitter into the capture card's HDMI input.
3. Confirm the OS sees the device.

**Windows** — list DirectShow devices:

```sh
ffmpeg -list_devices true -f dshow -i dummy
```

Note the exact device name (e.g. `USB Video`).

**Linux** — list V4L2 devices:

```sh
v4l2-ctl --list-devices
# or
ls /dev/video*
```

### Step 3: Test the capture path (optional but recommended)

Before a real transfer, record a few seconds and confirm you see the calibration
ring (white border with square markers in three corners) and the looping red start
frame.

**Windows** (PowerShell — not WSL):

```powershell
ffmpeg -y -rtbufsize 200M -f dshow -video_size 1920x1080 -framerate 30 -i video="USB Video" -c:v copy -t 10 "$env:USERPROFILE\Videos\capture_test.mp4"
```

**Linux** (adjust `/dev/video0` and input format as needed):

```sh
ffmpeg -f v4l2 -input_format mjpeg -video_size 1920x1080 -framerate 30 -i /dev/video0 -t 10 -r 30 capture_test.mp4
```

> On Windows, USB capture devices are often easier to access from native Windows
> than from WSL. For the full Windows capture + WSL extract workflow, see
> [runbook-windows-wsl.md](runbook-windows-wsl.md).

### Step 4: Record the live transfer

Start recording **before** or as the transmitter begins looping. Keep recording
until you have seen the red start frame **at least twice** (more loops improve
reliability when individual frames are dropped).

**Windows** (PowerShell — not WSL):

```powershell
ffmpeg -y -rtbufsize 200M -f dshow -video_size 1920x1080 -framerate 30 -i video="USB Video" -c:v copy "$env:USERPROFILE\Videos\captured.mp4"
```

Press `q` to stop after **60–90 seconds** (several loops). Watch for `frame dropped!`
— if it appears, re-capture with USB 3.0 and no other apps using the device.

**Linux:**

```sh
ffmpeg -f v4l2 -input_format mjpeg -video_size 1920x1080 -framerate 30 -i /dev/video0 -r 30 captured.mp4
```

The `-r` (frame rate) and `-s` / `-video_size` values must match the transmitter
settings.

### Step 5: Extract the file from the capture

> **WSL users:** OpenCV often cannot read USB-capture MJPEG directly. Convert
> the Windows capture to FFV1 first, then extract — see
> [runbook-windows-wsl.md](runbook-windows-wsl.md), Part C3–C4.

Run extract with the **same** width, height, fps, algo, levels, and size used
during inject:

```sh
hdmifiletransporter \
  -m extract \
  -i captured.mp4 \
  -o recovered.zip \
  --width 1920 --height 1080 \
  --fps 30 \
  --algo quantized --levels 2 \
  --size 6 \
  -p true
```

The tool:

1. Locates calibration markers in each frame and warps the image back to the
   encoded grid.
2. Verifies a per-frame CRC; invalid frames are skipped.
3. Reassembles pages until the full file is recovered.

If progress stalls, record more loops from the transmitter and run extract again
on a longer capture.

### Step 6: Verify the recovered file

Compare checksums or test the archive:

```sh
# Linux / macOS
sha256sum myfile.zip recovered.zip

# Or for a zip
unzip -t recovered.zip
```

The recovered file should be **byte-identical** to the original when enough clean
frames were captured.

---

## End-to-end checklist

Use this as a quick run-through for both sides.

### Transmitter

- [ ] Shared config chosen and recorded
- [ ] `hdmifiletransporter` installed
- [ ] File injected into `transfer.mkv` with matching flags
- [ ] HDMI connected; display set to encoded resolution
- [ ] Video playing fullscreen in a **loop**
- [ ] Playback left running until receiver confirms capture

### Receiver

- [ ] Same shared config as transmitter
- [ ] OpenCV, ffmpeg, and `hdmifiletransporter` installed
- [ ] Capture card detected; test clip looks correct
- [ ] Live capture recorded at matching resolution and fps
- [ ] At least two red start frames seen in the recording
- [ ] Extract run with identical inject flags
- [ ] Output file verified (checksum or functional test)

---

## Troubleshooting

| Symptom | Likely cause | What to try |
|---------|--------------|-------------|
| No calibration markers in capture | Wrong input, cable, or resolution | Re-check HDMI path; match `1920×1080` (or your chosen size) on display and capture |
| Extract finds no valid frames | Algo/size/fps mismatch | Confirm every flag matches inject exactly |
| Progress very slow or stuck | Dense encoding + noisy link | Re-inject with `--algo bw` or `--algo brightness --levels 4`; record more loops |
| Garbled output | Lossy intermediate video | Re-capture; avoid re-encoding the capture with heavy compression before extract |
| Device not found (Windows/WSL) | WSL cannot access USB video | Use native Windows for `ffmpeg` and extract |
| Red frame never appears | Playback not looping or wrong file | Enable loop; confirm the transmitter plays `transfer.mkv` |

---

## Further reading

- CLI option reference: [readme.md](../readme.md)
- Encoding modes, benchmarks, and presets: [findings.md](findings.md)
- Blog series (concept and HDMI experiments): links in [readme.md](../readme.md)
