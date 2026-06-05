# HDMI File Transfer — Runbook (Source + Windows + WSL)

Step-by-step commands for a real transfer using the setup validated in this
project. Use the **same configuration on every step**.

## Shared configuration (write this down)

Use these values for **inject**, **capture**, and **extract**:

```
--width 1920 --height 1080 --fps 30 --algo quantized --levels 2 --size 6
```

| Setting | Value |
|---------|-------|
| Resolution | 1920 × 1080 |
| Frame rate | **30 fps** (must match everywhere; typical for USB capture cards) |
| Algorithm | `quantized` |
| Levels | `2` |
| Cell size | `6` |

If extract keeps failing after a clean capture, re-inject with a more robust
preset and use the **same new flags** for capture and extract:

```
--algo brightness --levels 4 --size 6
```

---

## Part A — Source machine (transmitter)

Prepares `transfer.mkv` and plays it on HDMI in a loop.

### A1. Install Rust (if needed)

```sh
rustup toolchain install stable
rustup default stable
```

### A2. Install or build `hdmifiletransporter`

From crates.io:

```sh
cargo install hdmifiletransporter
```

Or from this repository:

```sh
git clone https://github.com/MrDesjardins/hdmifiletransporter.git
cd hdmifiletransporter
cargo build --release
# binary: target/release/hdmifiletransporter
```

### A3. Inject the file into a video

Replace `myfile.zip` with your file. Output must be **lossless** `.mkv`.

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

Copy `transfer.mkv` to the transmitter if you built on another machine.

### A4. Connect HDMI and set display

1. HDMI out from transmitter → capture card HDMI in (on the receiver).
2. Set the display to **1920 × 1080** (same as inject).

### A5. Play the video in a loop (fullscreen)

```sh
mpv --loop=inf --fullscreen transfer.mkv
```

Or VLC:

```sh
vlc --loop transfer.mkv
```

Leave playback running until the receiver has finished capturing.

**Tips:** fullscreen, no overlays, disable sleep/dimming.

---

## Part B — Windows receiver (capture only)

Run these in **PowerShell** or **Command Prompt** on Windows — **not** in WSL.
WSL cannot access the USB capture card reliably.

### B1. Install ffmpeg (if needed)

```powershell
winget install -e --id Gyan.FFmpeg.Essentials
```

Close and reopen the terminal, then verify:

```powershell
ffmpeg -version
```

### B2. List the capture device

```powershell
ffmpeg -list_devices true -f dshow -i dummy
```

Note the video device name (usually `"USB Video"`). Use the friendly name unless
you have multiple devices with the same name.

### B3. Quick capture test (~10 s)

Confirm HDMI signal is present **before** a long capture. No `frame dropped!`
messages should appear.

```powershell
ffmpeg -y -rtbufsize 200M -f dshow -video_size 1920x1080 -framerate 30 -i video="USB Video" -c:v copy -t 10 "$env:USERPROFILE\Videos\capture_test.mp4"
```

Play the test file in VLC. You should see the calibration border and encoded
pattern (red start frame if `transfer.mkv` is looping).

### B4. Record the live transfer (~60–90 s)

**Important:**

- Capture at **30 fps** (same as inject).
- Use **`-c:v copy`** — do **not** use FFV1 during live capture (too slow,
  causes buffer overflow and dropped frames).
- Save under your user folder (not `C:\` root).
- Watch the terminal: **no `frame dropped!` lines**.

```powershell
ffmpeg -y -rtbufsize 200M -f dshow -video_size 1920x1080 -framerate 30 -i video="USB Video" -c:v copy "$env:USERPROFILE\Videos\captured.mp4"
```

Press `q` to stop after **60–90 seconds** (several loops of the source video).

### B5. If you see `frame dropped!` during capture

Stop and retry:

1. Plug the capture card into a **USB 3.0** port (not a hub).
2. Close OBS, Camera, and any other app using the device.
3. Confirm the transmitter is playing **fullscreen** at 1920×1080.
4. Run the B4 command again.

---

## Part C — WSL receiver (convert + extract)

Run these in your **WSL terminal**. The Windows capture file is available at
`/mnt/c/Users/<WindowsUsername>/Videos/`.

Replace `<WindowsUsername>` with your Windows login (e.g. `miste`).

### C1. Install dependencies (if needed)

```sh
sudo apt update
sudo apt install -y libopencv-dev clang libclang-dev cmake ffmpeg
rustup toolchain install stable
rustup default stable
```

### C2. Build `hdmifiletransporter` (if needed)

```sh
cd ~/code/hdmifiletransporter
cargo build --release
```

### C3. Convert capture to FFV1 for OpenCV

OpenCV on Linux often cannot read USB-capture MJPEG in MP4/MKV directly.
Convert **after** capture (offline — not during recording):

```sh
ffmpeg -y -hide_banner -loglevel error \
  -fflags +genpts+igndts -err_detect ignore_err \
  -i /mnt/c/Users/<WindowsUsername>/Videos/captured.mp4 \
  -c:v ffv1 \
  /mnt/c/Users/<WindowsUsername>/Videos/captured_clean.mkv
```

**Expect many MJPEG errors during this step.** Lines like these are normal when
the USB capture contained corrupt or dropped frames — ffmpeg skips them and
keeps going:

```
Error submitting packet to decoder: Invalid data found when processing input
No JPEG data found in image
Found EOI before any SOF, ignoring
```

`-err_detect ignore_err` is what makes conversion continue past bad packets.
The error spam does **not** mean the command failed. **Ignore the noise; check
the final summary line** (run with `-loglevel info` if you want to see it):

```
frame= 1800 fps= ... time=00:01:00.00 ...
```

#### Pass / fail check

Get capture duration (seconds), then compare to decoded frame count:

```sh
# Duration of the Windows capture (seconds)
ffprobe -v error -show_entries format=duration \
  -of default=noprint_wrappers=1:nokey=1 \
  /mnt/c/Users/<WindowsUsername>/Videos/captured.mp4

# Decoded frames in the cleaned file (look at the last "frame=" line)
ffmpeg -hide_banner -i /mnt/c/Users/<WindowsUsername>/Videos/captured_clean.mkv -f null - 2>&1 | grep '^frame='
```

| Metric | Formula / target |
|--------|------------------|
| Expected frames | `duration × 30` (for 30 fps capture) |
| Good capture | Decoded frames ≥ **80%** of expected |
| Bad capture | Decoded frames ≪ expected (e.g. 600 of 2070) → **re-capture in Part B** |

Example: 69 s capture → expect ~2070 frames. Only **600** decoded means ~30%
survived — **re-capture on Windows** (watch for `frame dropped!` during B4).

### C4. Extract the file

Use the **same flags as inject**:

```sh
~/code/hdmifiletransporter/target/release/hdmifiletransporter \
  -m extract \
  -i /mnt/c/Users/<WindowsUsername>/Videos/captured_clean.mkv \
  -o /mnt/c/Users/<WindowsUsername>/Videos/recovered.zip \
  --width 1920 --height 1080 \
  --fps 30 \
  --algo quantized --levels 2 \
  --size 6 \
  -p true
```

**Success indicators:**

- `Start frame found with data size of ...`
- `Relevant (unique, valid) data frames:` close to **~150** for a ~3 MB file
  (depends on file size)
- No panic at the end; `recovered.zip` is written

### C5. Verify the recovered file

```sh
sha256sum myfile.zip /mnt/c/Users/<WindowsUsername>/Videos/recovered.zip
```

Or test a zip:

```sh
unzip -t /mnt/c/Users/<WindowsUsername>/Videos/recovered.zip
```

---

## Quick reference — which terminal where

| Step | Machine | Shell |
|------|---------|-------|
| Inject | Source | Linux / macOS / WSL |
| Play `transfer.mkv` | Source (transmitter) | Any player, fullscreen + loop |
| Capture (`ffmpeg -f dshow`) | Receiver | **Windows PowerShell** |
| Convert + extract | Receiver | **WSL** |

**Do not** run `wsl ...` inside WSL. If you are already in WSL, run the binary
directly. Use `wsl` only when invoking from Windows PowerShell.

---

## One-page checklist

### Source (transmitter)

- [ ] Injected `transfer.mkv` with shared config (`1920×1080`, `30 fps`, `quantized`, `levels 2`, `size 6`)
- [ ] HDMI connected; display set to 1920×1080
- [ ] `transfer.mkv` playing **fullscreen** in a **loop**

### Windows (capture)

- [ ] `ffmpeg` installed on Windows (not WSL)
- [ ] Device listed: `video="USB Video"`
- [ ] Test clip looks correct
- [ ] Live capture: **30 fps**, **`-c:v copy`**, **`-rtbufsize 200M`**, saved to `%USERPROFILE%\Videos\captured.mp4`
- [ ] No `frame dropped!` during recording

### WSL (extract)

- [ ] `captured.mp4` converted to `captured_clean.mkv` (FFV1)
- [ ] Decoded frame count is reasonable (~30 × seconds recorded)
- [ ] Extract run with **identical** inject flags
- [ ] `recovered.zip` checksum or `unzip -t` passes

---

## Troubleshooting

| Symptom | Likely cause | Fix |
|---------|--------------|-----|
| `Unknown input format: dshow` | Running ffmpeg in WSL/Linux | Use Windows PowerShell |
| `Permission denied` on `C:\capture.mkv` | Writing to drive root | Save to `%USERPROFILE%\Videos\` |
| `frame dropped!` during capture | FFV1 live encode or slow USB | Use `-c:v copy`, USB 3.0, close other apps |
| `Initial Frames count: 0` on extract | OpenCV can't read MJPEG capture | Run **C3** (convert to FFV1) first |
| `dup=...` in ffmpeg output | Capture fps ≠ source fps | Capture at **30 fps** if inject used 30 |
| Partial pages / panic on extract | Corrupt capture or fragile algo | Re-capture cleanly; try `--algo brightness --levels 4 --size 6` |
| `No JPEG data found` at capture start | Normal during device init | OK if recording continues and test clip plays |
| Many MJPEG errors during **C3 convert** | Corrupt packets in capture; `ignore_err` skips them | Normal noise — check final `frame=` count vs `duration × 30` |
| Low frame count after C3 | Dropped frames during Windows capture | Re-capture with B4; no `frame dropped!` lines |

---

## Example paths (user: `miste`)

| Location | Path |
|----------|------|
| Windows capture | `C:\Users\miste\Videos\captured.mp4` |
| WSL path to same file | `/mnt/c/Users/miste/Videos/captured.mp4` |
| Converted for extract | `/mnt/c/Users/miste/Videos/captured_clean.mkv` |
| Recovered output | `/mnt/c/Users/miste/Videos/recovered.zip` |
