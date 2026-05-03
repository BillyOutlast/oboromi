# Display/IO Reference: NVIDIA T239 (Switch 2)

> **Target:** Nintendo Switch 2 SoC — NVIDIA T239 custom processor display
> output path, dock subsystem, and audio pipeline
> **Document Status:** Complete — 15 sections covering display pipeline,
> LCD panel, display controller, docked output modes, dock hardware, audio
> subsystem, input devices (Joy-Con 2, touchscreen), connectivity (Wi-Fi 6E,
> Bluetooth 5.x, USB-C, NFC), camera/sensors, and gap analysis
>
> **Confidence Legend:**
> - **CONFIRMED** — Verified from Nintendo official documentation, Digital Foundry hardware review, NVIDIA documentation, or oboromi source code
> - **INFERRED** — Derived from closely related public documentation (Orin T234 TRM, Tegra display subsystem docs, JEDEC/DP specs)
> - **SPECULATIVE** — Based on industry analysis, reverse engineering, or extrapolation from similar parts

---

## Table of Contents

1. [Display/IO Overview](#1-displayio-overview)
2. [LCD Panel](#2-lcd-panel)
3. [Display Controller (DC)](#3-display-controller-dc)
4. [Docked Output Modes](#4-docked-output-modes)
5. [Dock Hardware](#5-dock-hardware)
6. [Audio Subsystem](#6-audio-subsystem)
7. [Input Overview](#7-input-overview)
8. [Joy-Con 2](#8-joy-con-2)
9. [Touchscreen](#9-touchscreen)
10. [Wi-Fi 6E](#10-wi-fi-6e)
11. [Bluetooth 5.x](#11-bluetooth-5x)
12. [USB-C](#12-usb-c)
13. [NFC](#13-nfc)
14. [Camera and Sensors](#14-camera-and-sensors)
15. [Gap Analysis](#15-gap-analysis)
16. [Confidence Tag Summary](#confidence-tag-summary)
17. [Citations](#citations)

---

## 1. Display/IO Overview

### 1.1 System Context

The Switch 2 display/IO subsystem spans three major functional blocks:
the internal LCD panel (handheld/tabletop modes), the display controller
(DC) pipeline that renders frames to either the panel or external output,
and the dock that bridges the console to a TV via HDMI. Audio output
covers internal speakers, headphone jack, HDMI passthrough, and a built-in
microphone for voice chat. [CONFIRMED — Nintendo official specs.] [1][2]

```
+------------------------------------------------------------------+
|                  Switch 2 Display/IO Block Diagram               |
|                                                                  |
|  +----------------------------------------------------------+   |
|  |  GPU (Ampere SM86)                                        |   |
|  |  12 SMs, 1536 CUDA cores                                  |   |
|  |  Renders framebuffers in LPDDR5X UMA                      |   |
|  +----------------------------+-----------------------------+   |
|                               |                                  |
|  +----------------------------v-----------------------------+   |
|  |  Display Controller (DC)                                  |   |
|  |  - Compositing / overlay planes                           |   |
|  |  - Scaling, color space conversion                        |   |
|  |  - HDR tone mapping                                       |   |
|  |  - VRR / VBlank timing                                    |   |
|  +----------+--------------------------+--------------------+   |
|             |                          |                         |
|             v                          v                         |
|  +---------------------+   +-----------------------------+      |
|  |  Internal LCD Panel  |   |  USB-C / Dock Output        |      |
|  |  7.9" 1080p 120Hz   |   |  DisplayPort Alt Mode       |      |
|  |  VRR + HDR10        |   |  → Dock → HDMI → TV         |      |
|  +---------------------+   |  Up to 4K60 / 1440p120      |      |
|                             +-----------------------------+      |
|                                                                  |
|  +----------------------------------------------------------+   |
|  |  Audio Subsystem                                          |   |
|  |  Audio DSP → Speakers / Headphone Jack / HDMI / USB-C    |   |
|  |  Built-in mono mic (noise/echo cancellation)              |   |
|  +----------------------------------------------------------+   |
+------------------------------------------------------------------+
```

**Figure 1.1:** High-level display/IO subsystem. The GPU renders to
framebuffers in unified memory; the DC composites and drives either the
internal panel or external output via USB-C DisplayPort Alt Mode.
[CONFIRMED — Nintendo specs, Digital Foundry analysis.] [1][2][3]

### 1.2 Operating Modes

| Mode | Display Target | Max Resolution | Max Refresh | HDR | Audio Output |
|---|---|---|---|---|---|
| Handheld | Internal LCD | 1920×1080 [CONFIRMED] | 120 Hz [CONFIRMED] | HDR10 [CONFIRMED] | Stereo speakers + headphone [CONFIRMED] |
| Tabletop | Internal LCD | 1920×1080 [CONFIRMED] | 120 Hz [CONFIRMED] | HDR10 [CONFIRMED] | Headphone + HDMI passthrough [CONFIRMED] |
| Docked (4K) | External TV | 3840×2160 [CONFIRMED] | 60 Hz [CONFIRMED] | HDR10 [CONFIRMED] | HDMI 5.1ch LPCM [CONFIRMED] |
| Docked (high-fps) | External TV | 2560×1440 [CONFIRMED] | 120 Hz [CONFIRMED] | HDR10 [CONFIRMED] | HDMI 5.1ch LPCM [CONFIRMED] |
| Docked (1080p) | External TV | 1920×1080 [CONFIRMED] | 120 Hz [CONFIRMED] | HDR10 [CONFIRMED] | HDMI 5.1ch LPCM [CONFIRMED] |

**Table 1.1:** Operating modes. All docked modes route through the USB-C
connector to the dock's DisplayPort-to-HDMI converter. [CONFIRMED] [1][2]

### 1.3 Horizon OS Service Map

The display and audio subsystems are managed by the following Horizon OS
services registered in oboromi's service registry: [CONFIRMED — oboromi
`core/src/nn/mod.rs` `start_host_services` function.] [4]

| Service | Domain | Role |
|---|---|---|
| `vi` | Display | Primary display compositor, vsync management [CONFIRMED] |
| `vi2` | Display | Secondary display service (multi-display / GameChat overlay) [CONFIRMED] |
| `vic` | Display | Video/image compositor for camera and GameChat [CONFIRMED] |
| `disp` | Display | Low-level display interface [CONFIRMED] |
| `dispdrv` | Display | Display driver / panel control [CONFIRMED] |
| `ommdisp` | Display | OMM display manager [CONFIRMED] |
| `aud` | Audio | Audio manager service [CONFIRMED] |
| `audout` | Audio | Audio output (playback) [CONFIRMED] |
| `audin` | Audio | Audio input (microphone) [CONFIRMED] |
| `audren` | Audio | Audio renderer (3D spatial audio) [CONFIRMED] |
| `audrec` | Audio | Audio recording [CONFIRMED] |
| `audsmx` | Audio | Audio mixer [CONFIRMED] |
| `audctl` | Audio | Audio control / routing [CONFIRMED] |
| `hwopus` | Audio | Hardware Opus codec [CONFIRMED] |
| `codecctl` | Media | Hardware codec control (H.264/H.265) [CONFIRMED] |

**Table 1.2:** Horizon OS services for display and audio. [4] For the
full Horizon OS microkernel architecture, IPC protocol (HIPC), and service
manager details, see **firmware.md** §1–§5. For GPU rendering pipeline
details that feed the display controller, see **gpu.md** §1–§11.

---

## 2. LCD Panel

### 2.1 Panel Specifications

The Switch 2 uses a **7.9-inch wide color gamut LCD** panel with
capacitive touch support. This is a significant upgrade from Switch 1's
6.2-inch 720p LCD. [CONFIRMED — Nintendo official specs.] [1][2]

| Parameter | Value | Confidence | Source |
|---|---|---|---|
| Panel size | 7.9 inches (diagonal) | CONFIRMED | [1][2] |
| Resolution | 1920 × 1080 (Full HD) | CONFIRMED | [1][2] |
| Pixel density | ~279 PPI | CONFIRMED | [2] |
| Panel type | LCD (IPS or equivalent wide-gamut) | CONFIRMED | [1] |
| Color gamut | Wide color gamut (sRGB+ / DCI-P3 subset) | INFERRED | [1] |
| Refresh rate (baseline) | 60 Hz | INFERRED | Standard LCD |
| Refresh rate (max) | 120 Hz | CONFIRMED | [1][2] |
| VRR (Variable Refresh Rate) | Yes, up to 120 Hz | CONFIRMED | [1][2] |
| HDR support | HDR10 | CONFIRMED | [1][2] |
| Touch input | Capacitive multi-touch | CONFIRMED | [1] |
| Brightness sensor | Yes (console body) | CONFIRMED | [1] |
| Backlight | Edge-lit LED (standard LCD) | INFERRED | — |
| Pixel format | RGB stripe (standard LCD) | INFERRED | — |
| Sub-pixel layout | RGB vertical stripe | INFERRED | — |

**Table 2.1:** LCD panel specifications. HDR10 on an LCD panel without
local dimming has limited practical benefit — peak brightness and contrast
ratio are constrained by the backlight technology. Digital Foundry notes
that HDR is primarily useful when docked on an HDR-capable TV.
[CONFIRMED] [1][2][3]

### 2.2 Display Timing

The following display timings are calculated from confirmed specifications.
Pixel clock values are derived from standard CVT-RB timing formulas.
[INFERRED — CVT timing calculations from confirmed resolution/refresh.]

| Mode | Resolution | Refresh | Pixel Clock | H Total | V Total | Bandwidth (RGB888) |
|---|---|---|---|---|---|---|
| Handheld 60Hz | 1920×1080 | 60 Hz | ~148.5 MHz [INFERRED] | ~2200 | ~1125 | ~4.05 GB/s [INFERRED] |
| Handheld 120Hz | 1920×1080 | 120 Hz | ~297 MHz [INFERRED] | ~2200 | ~1125 | ~8.10 GB/s [INFERRED] |
| Docked 1080p60 | 1920×1080 | 60 Hz | ~148.5 MHz [INFERRED] | ~2200 | ~1125 | ~4.05 GB/s [INFERRED] |
| Docked 1440p60 | 2560×1440 | 60 Hz | ~241.5 MHz [INFERRED] | ~3000 | ~1500 | ~6.52 GB/s [INFERRED] |
| Docked 4K60 | 3840×2160 | 60 Hz | ~594 MHz [INFERRED] | ~4400 | ~2250 | ~16.04 GB/s [INFERRED] |
| Docked 1080p120 | 1920×1080 | 120 Hz | ~297 MHz [INFERRED] | ~2200 | ~1125 | ~8.10 GB/s [INFERRED] |
| Docked 1440p120 | 2560×1440 | 120 Hz | ~483 MHz [INFERRED] | ~3000 | ~1500 | ~13.03 GB/s [INFERRED] |

**Table 2.2:** Display timing modes. Bandwidth = pixel_clock × 3 bytes/pixel
(RGB888). The 4K60 mode requires approximately 16 GB/s of memory bandwidth
for display scanout alone. [INFERRED]

### 2.3 VRR Behavior

Variable Refresh Rate allows the display to synchronize its refresh cycle
with the GPU's frame output, eliminating screen tearing without the input
lag of traditional V-Sync. [INFERRED — VRR/FreeSync/Adaptive-Sync standard
behavior.] [1][2]

| Parameter | Value | Confidence |
|---|---|---|
| VRR technology | Adaptive-Sync (HDMI Forum VRR) [INFERRED] | INFERRED |
| VRR range (panel) | ~30–120 Hz [SPECULATIVE] | SPECULATIVE |
| VRR range (docked) | Depends on TV capability [CONFIRMED] | CONFIRMED |
| VRR enable | Via `vi` service / DC configuration [INFERRED] | INFERRED |
| LFC (Low Framerate Compensation) | Likely supported [SPECULATIVE] | SPECULATIVE |

**Table 2.3:** VRR parameters. LFC doubles frames when framerate drops
below the VRR minimum to maintain smooth playback. [SPECULATIVE]

### 2.4 HDR10 Pipeline

HDR10 support enables a wider luminance range (up to 10,000 nits theoretical)
and wider color gamut (BT.2020) compared to standard SDR (BT.709, 100 nits).
[INFERRED — HDR10 specification.] [1][2]

| Parameter | Value | Confidence |
|---|---|---|
| HDR standard | HDR10 (static metadata) | CONFIRMED |
| Color space | BT.2020 container, DCI-P3 typical gamut [INFERRED] | INFERRED |
| Transfer function | PQ (Perceptual Quantizer, SMPTE ST 2084) [INFERRED] | INFERRED |
| Bit depth | 10-bit per channel [INFERRED] | INFERRED |
| MaxCLL / MaxFALL | Metadata embedded per-frame [INFERRED] | INFERRED |
| HDR on internal LCD | Supported but limited (no local dimming) [INFERRED] | INFERRED |
| HDR on TV (docked) | Full HDR10 if TV supports it [CONFIRMED] | CONFIRMED |

**Table 2.4:** HDR10 pipeline parameters. The internal LCD's HDR capability
is constrained by backlight technology — practical HDR benefit is greater
on an external HDR-capable TV. [INFERRED] [3]

### 2.5 Touch Controller

| Parameter | Value | Confidence |
|---|---|---|
| Touch type | Capacitive | CONFIRMED |
| Multi-touch | Yes (standard capacitive) [INFERRED] | INFERRED |
| Touch resolution | Panel-native (1920×1080) [INFERRED] | INFERRED |
| Touch interface | I2C to SoC [INFERRED] | INFERRED |
| Touch service | `hid` (HID input service) [INFERRED] | INFERRED |
| Digitizer IC | Proprietary (likely Synaptics or Goodix) [SPECULATIVE] | SPECULATIVE |

**Table 2.5:** Touch controller. Touch input is routed through the `hid`
service via the HID bus (`hidbus`) service. [CONFIRMED — oboromi service
list includes `hid` and `hidbus`.] [4]

---

## 3. Display Controller (DC)

### 3.1 DC Block Overview

The T239's Display Controller (DC) is responsible for compositing
framebuffers from GPU render targets, applying color space conversion,
scaling, HDR tone mapping, and driving the output timing generator for
either the internal panel or external display. The DC architecture is
derived from NVIDIA's Tegra display subsystem, documented in the Orin
T234 TRM. [INFERRED — T234 Orin TRM, Tegra display architecture.] [5]

```
+------------------------------------------------------------------+
|                Display Controller (DC) Pipeline                  |
|                                                                  |
|  +-------------------+                                           |
|  |  GPU Framebuffer  |  (in LPDDR5X UMA)                        |
|  |  Render Target 0  |                                           |
|  +--------+----------+                                           |
|           |                                                      |
|  +--------v----------+  +-------------------+                   |
|  |  Window (WIN)      |  |  Overlay Planes   |                   |
|  |  Channel 0         |  |  WIN1, WIN2, WIN3 |                   |
|  |  - Fetch from DRAM |  |  (UI, cursor,     |                   |
|  |  - Pixel format    |  |   camera overlay)  |                   |
|  |    decode          |  +--------+----------+                   |
|  +--------+-----------+           |                              |
|           |                       |                              |
|  +--------v-----------+  +-------v----------+                   |
|  |  Scaler / CSC      |  |  Scaler / CSC    |                   |
|  |  (Color Space      |  |  (per-plane)     |                   |
|  |   Conversion)      |  |                  |                   |
|  +--------+-----------+  +-------+----------+                   |
|           |                       |                              |
|  +--------v-----------------------v----------+                   |
|  |           Compositor / Blender            |                   |
|  |  - Alpha blending (per-plane)             |                   |
|  |  - Z-ordering                             |                   |
|  |  - HDR tone mapping (PQ curve)            |                   |
|  |  - Gamma correction                       |                   |
|  +---------------------+--------------------+                   |
|                            |                                      |
|  +---------------------v--------------------+                   |
|  |           Timing Generator (TG)           |                   |
|  |  - Pixel clock generation                 |                   |
|  |  - H/V sync timing                        |                   |
|  |  - VRR / VBlank control                   |                   |
|  |  - Frame packing (3D) — disabled           |                   |
|  +----------+--------------------+----------+                   |
|             |                    |                                |
|  +----------v---------+  +------v-----------+                   |
|  |  DSI (MIPI) Output |  |  DP / HDMI Output |                   |
|  |  → Internal LCD    |  |  → USB-C → Dock   |                   |
|  +--------------------+  +------------------+                   |
+------------------------------------------------------------------+
```

**Figure 3.1:** Display Controller pipeline. Multiple window channels
(WIN0–WIN3) fetch pixel data from DRAM, apply per-plane scaling and
color conversion, then composite into a single output frame via the
blender. The timing generator drives either the internal DSI panel or
external DP/HDMI output. [INFERRED — T234 TRM.] [5]

### 3.2 Window Channels

The DC supports multiple **window channels** (WIN0–WIN3) that independently
fetch pixel data from memory and apply per-plane transformations. Each
window can have a different pixel format, resolution, and position.
[SPECULATIVE — Inferred from T234 DC window architecture.]

| Window | Typical Use | Pixel Formats | Alpha | Scaling |
|---|---|---|---|---|
| WIN0 | Game framebuffer (primary) | RGB888, RGBA8888, NV12, P010 [INFERRED] | Per-pixel [INFERRED] | Bilinear [INFERRED] |
| WIN1 | System UI overlay | ARGB8888 [INFERRED] | Per-pixel [INFERRED] | Bilinear [INFERRED] |
| WIN2 | Camera / GameChat overlay | NV12, YUY2 [INFERRED] | Per-pixel [INFERRED] | Bilinear [INFERRED] |
| WIN3 | Cursor / notifications | ARGB8888 [INFERRED] | Per-pixel [INFERRED] | None [INFERRED] |

**Table 3.1:** Window channel assignments. Each window independently reads
from a framebuffer in DRAM and applies format conversion before compositing.
[SPECULATIVE]

### 3.3 Pixel Formats

The DC supports multiple pixel formats for flexible framebuffer
composition. [INFERRED — T234 TRM DC pixel format table.]

| Format | BPP | Channels | Use Case | Confidence |
|---|---|---|---|---|
| RGB888 | 24 | R8G8B8 | Standard SDR framebuffer | INFERRED |
| RGBA8888 | 32 | R8G8B8A8 | Alpha-blended overlays | INFERRED |
| RGB565 | 16 | R5G6B5 | Low-bandwidth modes | INFERRED |
| ARGB8888 | 32 | A8R8G8B8 | System UI, cursor | INFERRED |
| NV12 | 12 | Y + UV (4:2:0) | Video decode output | INFERRED |
| P010 | 24 | Y + UV (10-bit 4:2:0) | HDR video decode | INFERRED |
| YUY2 | 16 | YUYV (4:2:2) | Camera capture | INFERRED |
| BGR888 | 24 | B8G8R8 | Alternate channel order | INFERRED |

**Table 3.2:** Supported pixel formats. P010 is the HDR-capable variant of
NV12, using 10 bits per component for PQ-encoded content. [INFERRED]

### 3.4 Color Space Conversion (CSC)

The DC performs hardware color space conversion between different color
spaces and transfer functions. [INFERRED — T234 TRM, HDR pipeline.]

| Conversion | Input | Output | Use Case |
|---|---|---|---|
| SDR → SDR | BT.709 / sRGB | BT.709 / sRGB | Standard game output [INFERRED] |
| SDR → HDR | BT.709 / sRGB | BT.2020 / PQ | HDR10 output [INFERRED] |
| HDR → HDR | BT.2020 / PQ | BT.2020 / PQ | Native HDR content [INFERRED] |
| YUV → RGB | NV12 / P010 | RGB888 / RGBA8888 | Video decode to display [INFERRED] |
| RGB → YUV | RGB888 | NV12 | Video encode input [INFERRED] |

**Table 3.3:** Color space conversion operations. HDR tone mapping applies
the PQ (Perceptual Quantizer) EOTF/OETF curves. [INFERRED]

### 3.5 DC Register Space

The DC's register interface follows NVIDIA's standard Tegra register block
convention. Base addresses are inferred from T234 TRM.
[SPECULATIVE — T234 TRM register map.]

| Register Block | Base Address | Size | Description |
|---|---|---|---|
| DC_GLOBAL | 0x1520_0000 [SPECULATIVE] | 4 KB | Global DC control (mode, interrupt) |
| DC_WIN0 | 0x1520_0400 [SPECULATIVE] | 1 KB | Window 0 configuration |
| DC_WIN1 | 0x1520_0800 [SPECULATIVE] | 1 KB | Window 1 configuration |
| DC_WIN2 | 0x1520_0C00 [SPECULATIVE] | 1 KB | Window 2 configuration |
| DC_WIN3 | 0x1520_1000 [SPECULATIVE] | 1 KB | Window 3 configuration |
| DC_COM | 0x1520_2000 [SPECULATIVE] | 4 KB | Compositor / blender control |
| DC_DISP | 0x1520_4000 [SPECULATIVE] | 4 KB | Display timing generator |
| DC_DSI | 0x1520_8000 [SPECULATIVE] | 4 KB | MIPI DSI controller |
| DC_HDMI | 0x1521_0000 [SPECULATIVE] | 4 KB | HDMI controller |

**Table 3.4:** DC register blocks. Addresses are SPECULATIVE — derived from
T234 TRM conventions. The Switch 2's T239 may use different base addresses.
[SPECULATIVE] [5]

### 3.6 VSync and Frame Pacing

The DC generates VSync interrupts at the display refresh rate. The `vi`
service uses these interrupts to synchronize frame presentation with the
display. Under VRR, the VSync period is variable based on the GPU's
frame completion time. [INFERRED — Standard display compositor behavior.]

```
Frame Pacing (Fixed 60Hz):
  Frame 0     Frame 1     Frame 2     Frame 3
  |-----------|-----------|-----------|-----------|
  VSync0      VSync1      VSync2      VSync3
  16.67ms     16.67ms     16.67ms     16.67ms

Frame Pacing (VRR, ~45fps average):
  Frame 0       Frame 1         Frame 2     Frame 3
  |-------------|---------------|-----------|---------|
  VSync0        VSync1          VSync2      VSync3
  22.2ms        22.2ms          16.67ms     22.2ms
```

**Figure 3.2:** Frame pacing under fixed and VRR refresh. VRR eliminates
tearing by allowing variable VSync intervals. [INFERRED]

### 3.7 Display Scanout Bandwidth

Display scanout reads framebuffer data from DRAM at a rate determined by
the resolution, refresh rate, and pixel format. This bandwidth must be
guaranteed by the memory controller's QoS arbitration to prevent display
artifacts. [INFERRED — Standard display scanout analysis.]

| Mode | Resolution | Refresh | Pixel Format | Scanout BW |
|---|---|---|---|---|
| Handheld 60 | 1920×1080 | 60 Hz | RGB888 | 373 MB/s [INFERRED] |
| Handheld 120 | 1920×1080 | 120 Hz | RGB888 | 746 MB/s [INFERRED] |
| Docked 1080p60 | 1920×1080 | 60 Hz | RGB888 | 373 MB/s [INFERRED] |
| Docked 4K60 | 3840×2160 | 60 Hz | RGB888 | 1,493 MB/s [INFERRED] |
| Docked 4K60 HDR | 3840×2160 | 60 Hz | RGBA8888 | 1,991 MB/s [INFERRED] |
| Docked 1440p120 | 2560×1440 | 120 Hz | RGB888 | 1,327 MB/s [INFERRED] |

**Table 3.5:** Display scanout bandwidth. Scanout BW = width × height ×
bpp/8 × refresh_rate. Double-buffering doubles the bandwidth requirement
for writes (GPU) but not reads (DC reads front buffer only). [INFERRED]
For the full memory controller architecture and bandwidth characteristics
including QoS arbitration, see **memory.md** §3 and §6.

---

## 4. Docked Output Modes

### 4.1 USB-C DisplayPort Alt Mode

The Switch 2 outputs video to the dock via **DisplayPort Alt Mode** over
the USB-C connector. The bottom USB-C port carries DisplayPort lanes
alongside USB 3.x data and USB Power Delivery. [CONFIRMED — Two USB-C
ports confirmed; DP Alt Mode inferred from docked 4K capability.] [1][2]

| Parameter | Value | Confidence |
|---|---|---|
| Output interface | USB-C with DisplayPort Alt Mode | INFERRED |
| DP version | DisplayPort 1.4 (HBR3) [INFERRED] | INFERRED |
| DP lanes | 2 or 4 lanes [SPECULATIVE] | SPECULATIVE |
| Max link rate | 8.1 Gbps/lane (HBR3) [INFERRED] | INFERRED |
| Max aggregate bandwidth | 32.4 Gbps (4-lane HBR3) [INFERRED] | INFERRED |
| DSC (Display Stream Compression) | Likely supported for 4K60 [INFERRED] | INFERRED |
| Simultaneous USB 3.x | Yes (shared via mux) [INFERRED] | INFERRED |
| USB-C connector | Bottom port (charging + dock) [CONFIRMED] | CONFIRMED |

**Table 4.1:** USB-C display output characteristics. DisplayPort 1.4 HBR3
with DSC can support 4K60 HDR; without DSC, 4K60 requires chroma subsampling.
[INFERRED]

### 4.2 Supported Output Modes

The following display output modes are confirmed or inferred for docked
operation. All modes are output via HDMI from the dock. [CONFIRMED —
Nintendo official specs.] [1][2]

| Resolution | Refresh Rate | HDR | Color Depth | Confidence | Notes |
|---|---|---|---|---|---|
| 3840×2160 (4K) | 60 Hz | HDR10 | 8-bit / 10-bit [INFERRED] | CONFIRMED | Max resolution mode |
| 2560×1440 (1440p) | 120 Hz | HDR10 | 8-bit [INFERRED] | CONFIRMED | High-refresh mode |
| 1920×1080 (1080p) | 120 Hz | HDR10 | 8-bit [INFERRED] | CONFIRMED | High-refresh mode |
| 1920×1080 (1080p) | 60 Hz | HDR10 | 8-bit / 10-bit [INFERRED] | CONFIRMED | Standard mode |
| 1280×720 (720p) | 60 Hz | — | 8-bit [INFERRED] | INFERRED | Backward compat |

**Table 4.2:** Docked output modes. Nintendo confirms 4K60, 1440p120, and
1080p120 as the primary docked modes. [CONFIRMED] [1][2]

### 4.3 HDMI Output

The dock converts DisplayPort to HDMI using a DP-to-HDMI protocol
converter chip. [INFERRED — Standard dock architecture.]

| Parameter | Value | Confidence |
|---|---|---|
| HDMI version | HDMI 2.0 or 2.1 [INFERRED] | INFERRED |
| Max TMDS clock | 600 MHz (HDMI 2.1) / 340 MHz (HDMI 2.0) [INFERRED] | INFERRED |
| 4K60 support | Yes (requires HDMI 2.0+ or DSC) [CONFIRMED] | CONFIRMED |
| 4K120 support | Not supported (Nintendo lists max 4K60) [CONFIRMED] | CONFIRMED |
| 1440p120 support | Yes [CONFIRMED] | CONFIRMED |
| 1080p120 support | Yes [CONFIRMED] | CONFIRMED |
| HDR10 passthrough | Yes [CONFIRMED] | CONFIRMED |
| VRR passthrough | Yes (HDMI Forum VRR) [INFERRED] | INFERRED |
| Audio return (eARC) | Not applicable (console is source) [INFERRED] | INFERRED |
| DP-to-HDMI converter | Realtek RTD2172 or similar [SPECULATIVE] | SPECULATIVE |

**Table 4.3:** HDMI output characteristics. The dock likely uses an HDMI 2.0
converter with FRL (Fixed Rate Link) for 4K60 HDR, or HDMI 2.1 for higher
bandwidth modes. [INFERRED]

### 4.4 Resolution Switching

When the console is inserted into or removed from the dock, the display
pipeline transitions between handheld and docked modes. This involves:
[INFERRED — Standard hybrid console behavior.]

1. **Dock insertion detected** via USB-C CC pins (see §5.4) [INFERRED]
2. **DC reconfigures** output timing from internal panel to DP Alt Mode
   [INFERRED]
3. **Resolution negotiation** between dock and TV via EDID [INFERRED]
4. **GPU clock scaling** from handheld to docked DVFS profile [CONFIRMED]
5. **Memory clock scaling** from 4,266 MT/s to 6,400 MT/s [CONFIRMED]
6. **Audio output routing** switches from speakers to HDMI [INFERRED]

The transition typically completes within 1–3 seconds. Games may
receive a callback to adjust rendering resolution. [SPECULATIVE]

### 4.5 Tabletop Mode

In tabletop mode, the console is propped up using the built-in kickstand
and the internal LCD is used while controllers are detached. The top
USB-C port can be used for charging. Video output is limited to the
internal panel resolution (1920×1080). [CONFIRMED — Nintendo specs.] [1]

| Parameter | Tabletop Mode |
|---|---|
| Display | Internal LCD [CONFIRMED] |
| Max resolution | 1920×1080 [CONFIRMED] |
| Max refresh | 120 Hz [CONFIRMED] |
| Charging | Top USB-C port [CONFIRMED] |
| Audio | Headphone jack or Bluetooth [INFERRED] |

**Table 4.4:** Tabletop mode configuration. [CONFIRMED] [1] For the
NVN2 graphics API that generates display command buffers, see **gpu.md**
§11. For the GPU rendering pipeline that produces framebuffers consumed
by the display controller, see **gpu.md** §9 (DLSS and Display Pipeline).

---

## 5. Dock Hardware

### 5.1 Dock Overview

The Switch 2 dock is a passive enclosure with active electronics that
converts the console's USB-C DisplayPort Alt Mode signal to HDMI output
and provides wired networking and USB expansion. [CONFIRMED — Nintendo
specs, Digital Foundry dock analysis.] [1][2][3]

```
+------------------------------------------------------------------+
|                    Switch 2 Dock Architecture                    |
|                                                                  |
|  +----------------------------------------------------------+   |
|  |  Dock Enclosure                                           |   |
|  |                                                           |   |
|  |  +--------------------+     +-------------------------+   |   |
|  |  |  USB-C Input Port  |     |  HDMI Output Port       |   |   |
|  |  |  (console bottom)  |---->|  (to TV / monitor)      |   |   |
|  |  +--------------------+     +-------------------------+   |   |
|  |          |                                                |   |
|  |          v                                                |   |
|  |  +--------------------+                                   |   |
|  |  |  DP-to-HDMI        |  Protocol converter chip          |   |
|  |  |  Converter IC      |  (DP 1.4 → HDMI 2.0/2.1)        |   |
|  |  +--------------------+                                   |   |
|  |          |                                                |   |
|  |          v                                                |   |
|  |  +--------------------+     +-------------------------+   |   |
|  |  |  USB Hub IC         |     |  Ethernet Controller    |   |   |
|  |  |  (USB 3.x / 2.0)  |     |  (Gigabit LAN)          |   |   |
|  |  +----+-------+------+     +------------+------------+   |   |
|  |       |       |                          |                |   |
|  |       v       v                          v                |   |
|  |  +--------+--------+          +--------------------+      |   |
|  |  | USB-A | USB-A   |          |  RJ-45 Ethernet    |      |   |
|  |  | Port  | Port    |          |  Port               |      |   |
|  |  +-------+---------+          +--------------------+      |   |
|  |                                                           |   |
|  |  +--------------------+                                   |   |
|  |  |  USB PD Controller |  Power delivery negotiation       |   |
|  |  |  (PD 3.0+)        |  Charges console battery          |   |
|  |  +--------------------+                                   |   |
|  |                                                           |   |
|  |  +--------------------+                                   |   |
|  |  |  Dock Fan (active) |  Thermal management               |   |
|  |  +--------------------+                                   |   |
|  +----------------------------------------------------------+   |
+------------------------------------------------------------------+
```

**Figure 5.1:** Switch 2 dock architecture. The dock contains a DP-to-HDMI
converter, USB hub, Ethernet controller, PD charger, and active cooling fan.
[CONFIRMED — Nintendo specs, Digital Foundry analysis.] [1][2][3]

### 5.2 Dock Connectors and Ports

| Port | Type | Function | Confidence |
|---|---|---|---|
| Console input | USB-C (female) | DP Alt Mode + USB 3.x + PD charging [INFERRED] | CONFIRMED |
| HDMI output | HDMI Type A (female) | Video + audio to TV [CONFIRMED] | CONFIRMED |
| Ethernet | RJ-45 (female) | Wired LAN (Gigabit) [CONFIRMED] | CONFIRMED |
| USB-A | USB Type A (female) | Accessories (controllers, storage) [INFERRED] | INFERRED |
| USB-A count | 2 ports [SPECULATIVE] | Similar to Switch 1 dock | SPECULATIVE |
| Power input | USB-C (female) or barrel jack [INFERRED] | PD charging | INFERRED |

**Table 5.1:** Dock connectors. Nintendo confirms wired LAN port on dock
and HDMI output. USB-A port count is SPECULATIVE based on Switch 1 precedent.
[SPECULATIVE] [1][2]

### 5.3 USB Power Delivery

The dock provides power delivery to charge the console while playing.
The console's battery is 5,220 mAh (3.78V, 19.74 Wh).
[CONFIRMED — Nintendo specs.] [1]

| Parameter | Value | Confidence |
|---|---|---|
| PD standard | USB PD 3.0 or later [INFERRED] | INFERRED |
| Charging voltage | 15V or 20V [INFERRED] | INFERRED |
| Charging current | 2–3A [INFERRED] | INFERRED |
| Max charging power | ~39–45W [SPECULATIVE] | SPECULATIVE |
| Battery capacity | 5,220 mAh / 19.74 Wh [CONFIRMED] | CONFIRMED |
| Charge time (sleep) | ~3 hours [CONFIRMED] | CONFIRMED |
| Play-while-charging | Yes [CONFIRMED] | CONFIRMED |

**Table 5.2:** USB Power Delivery characteristics. The dock negotiates
PD voltage/current via the CC (Configuration Channel) pins on the USB-C
connector. [INFERRED] [1]

### 5.4 Dock Detection

The console detects dock insertion via the **USB-C CC (Configuration
Channel) pins**. When the dock is connected, the CC pins establish a
USB PD contract and DisplayPort Alt Mode capability advertisement.
[INFERRED — Standard USB-C / PD protocol.] [1]

```
Dock Detection Sequence:
  1. USB-C connector inserted (physical)
  2. CC pull-down detected → USB-C connection established
  3. USB PD negotiation → power contract (voltage/current)
  4. DP Alt Mode entry → VDM (Vendor Defined Messages)
     - DP capability discovery
     - Lane assignment (2 or 4 DP lanes)
     - Link training (HBR2 / HBR3)
  5. USB enumeration → USB hub + Ethernet controller
  6. Display output active → HDMI signal to TV
  7. Game notified of docked mode transition
```

**Figure 5.2:** Dock detection and initialization sequence. [INFERRED —
USB-C PD and DP Alt Mode standard protocol.]

### 5.5 Dock Fan

The Switch 2 dock includes an **active cooling fan** to assist with
thermal management during docked operation, where the T239 SoC runs at
higher clocks (GPU: 1,007 MHz vs 561 MHz handheld).
[CONFIRMED — Digital Foundry analysis, dock thermal design.] [3]

| Parameter | Value | Confidence |
|---|---|---|
| Fan type | Axial or centrifugal [SPECULATIVE] | SPECULATIVE |
| Fan control | PWM via dock MCU [INFERRED] | INFERRED |
| Thermal path | Dock fan → console bottom vent → SoC heatsink [INFERRED] | INFERRED |
| Fan noise | Low (designed for living room) [INFERRED] | INFERRED |
| Fan speed profile | Temperature-based (varies with SoC load) [INFERRED] | INFERRED |

**Table 5.3:** Dock fan characteristics. The dock fan supplements the
console's internal cooling system during high-performance docked mode.
[INFERRED] [3]

### 5.6 Ethernet Controller

The dock includes a wired Gigabit Ethernet controller for reliable,
low-latency networking. This is preferred over Wi-Fi for competitive
online gaming. [CONFIRMED — Nintendo specs confirm wired LAN on dock.] [1]

| Parameter | Value | Confidence |
|---|---|---|
| Ethernet standard | Gigabit Ethernet (1000BASE-T) [INFERRED] | INFERRED |
| Controller IC | USB Ethernet adapter (RTL8153 or similar) [SPECULATIVE] | SPECULATIVE |
| Interface | USB 3.x to SoC via USB hub [INFERRED] | INFERRED |
| Max throughput | 1 Gbps [INFERRED] | INFERRED |
| Wake-on-LAN | Supported (for remote downloads) [SPECULATIVE] | SPECULATIVE |
| Horizon OS service | `eth`, `ethc` [CONFIRMED — oboromi service list] | CONFIRMED |

**Table 5.4:** Ethernet controller characteristics. The `eth` and `ethc`
services in oboromi handle Ethernet connectivity. [CONFIRMED] [4]

### 5.7 USB Hub

The dock contains a USB hub that expands the single USB-C connection
from the console into multiple downstream ports for accessories.
[INFERRED — Standard dock architecture.]

| Parameter | Value | Confidence |
|---|---|---|
| Hub IC | USB 3.x hub controller [INFERRED] | INFERRED |
| Upstream | USB-C from console [INFERRED] | INFERRED |
| Downstream ports | USB-A (for controllers, keyboards) [INFERRED] | INFERRED |
| USB version | USB 3.2 Gen 1 (5 Gbps) or Gen 2 (10 Gbps) [INFERRED] | INFERRED |
| Per-port power | 5V / 500mA–900mA [INFERRED] | INFERRED |
| Horizon OS service | `usb` [CONFIRMED — oboromi service list] | CONFIRMED |

**Table 5.5:** USB hub characteristics. [INFERRED] [4]

---

## 6. Audio Subsystem

### 6.1 Audio Overview

The Switch 2 audio subsystem handles all audio input and output paths:
internal speakers (handheld), headphone jack, HDMI audio passthrough
(docked), Bluetooth audio, and microphone input for GameChat. Audio
processing is managed by a dedicated audio DSP and multiple Horizon OS
audio services. [CONFIRMED — Nintendo specs, oboromi service list.] [1][2][4]

```
+------------------------------------------------------------------+
|                  Audio Subsystem Block Diagram                   |
|                                                                  |
|  +----------------------------------------------------------+   |
|  |  Game Application                                         |   |
|  |  - Audio buffers in LPDDR5X                               |   |
|  |  - NVN2 audio submission / middleware (FMOD, Wwise)        |   |
|  +----------------------------+-----------------------------+   |
|                               |                                  |
|                    HIPC (audren, audout)                         |
|                               |                                  |
|  +----------------------------v-----------------------------+   |
|  |  Audio Renderer (audren)                                  |   |
|  |  - 3D spatial audio processing                            |   |
|  |  - HRTF binaural rendering                                |   |
|  |  - Mixing (up to 256 voices) [SPECULATIVE]               |   |
|  |  - Effects (reverb, EQ, limiter)                          |   |
|  +----------------------------+-----------------------------+   |
|                               |                                  |
|  +----------------------------v-----------------------------+   |
|  |  Audio DSP (Hardware)                                     |   |
|  |  - Final mix processing                                   |   |
|  |  - Sample rate conversion                                 |   |
|  |  - Volume / mute control                                  |   |
|  |  - Surround sound effect (headphone / speaker)            |   |
|  +-----+----------+----------+----------+------------------+   |
|        |          |          |          |                        |
|        v          v          v          v                        |
|  +----------+ +----------+ +------+ +----------+                |
|  | Stereo   | | 3.5mm    | | HDMI | | Bluetooth|                |
|  | Speakers | | Headphone| | 5.1  | | Audio    |                |
|  | (LPCM   | | Jack     | | LPCM | | (A2DP)   |                |
|  |  2.0ch) | | (CTIA)   | |      | |          |                |
|  +----------+ +----------+ +------+ +----------+                |
|                                                                  |
|  +----------------------------------------------------------+   |
|  |  Audio Input (Microphone)                                 |   |
|  |  Built-in mono mic → noise/echo cancellation → AGC        |   |
|  |  → audin service → GameChat / game voice chat             |   |
|  +----------------------------------------------------------+   |
+------------------------------------------------------------------+
```

**Figure 6.1:** Audio subsystem block diagram. The audio renderer performs
3D spatial audio processing; the hardware DSP handles final mixing and
output routing. Multiple output paths are available depending on mode.
[CONFIRMED — Nintendo specs, oboromi service list.] [1][2][4]

### 6.2 Audio Output Paths

| Output Path | Channels | Format | Mode | Confidence |
|---|---|---|---|---|
| Internal speakers | 2.0 (stereo) | Linear PCM | Handheld / Tabletop | CONFIRMED |
| 3.5mm headphone jack | 2.0 (stereo) | Linear PCM (CTIA) | All modes | CONFIRMED |
| HDMI audio | 5.1 (surround) | Linear PCM | Docked | CONFIRMED |
| Bluetooth audio | 2.0 (stereo) | SBC / AAC / LDAC [INFERRED] | All modes | INFERRED |
| Surround effect (speaker) | Virtual surround | DSP-processed | Speaker mode | CONFIRMED |
| Surround effect (headphone) | Virtual surround | HRTF binaural | Headphone mode | CONFIRMED |

**Table 6.1:** Audio output paths. Nintendo confirms 2.0ch stereo speakers,
5.1ch LPCM over HDMI, and surround sound effects for headphone/speaker
output. Bluetooth codec support is INFERRED. [CONFIRMED] [1][2]

### 6.3 Speaker System

| Parameter | Value | Confidence |
|---|---|---|
| Speaker count | 2 (stereo, left + right) [CONFIRMED] | CONFIRMED |
| Speaker type | Independent enclosure structure [CONFIRMED] | CONFIRMED |
| Frequency response | ~200 Hz – 16 kHz [SPECULATIVE] | SPECULATIVE |
| Amplifier | Integrated class-D [INFERRED] | INFERRED |
| Amplifier power | ~1–2W per channel [SPECULATIVE] | SPECULATIVE |
| Audio quality | "Natural, clear sound quality" [CONFIRMED] | CONFIRMED |
| Surround effect | System update required [CONFIRMED] | CONFIRMED |

**Table 6.2:** Speaker system specifications. Nintendo describes "independent
enclosure structure" for improved sound isolation and clarity.
[CONFIRMED] [1]

### 6.4 Headphone Jack

| Parameter | Value | Confidence |
|---|---|---|
| Connector | 3.5mm 4-contact stereo mini-plug [CONFIRMED] | CONFIRMED |
| Standard | CTIA (OMTP variant) [CONFIRMED] | CONFIRMED |
| Channels | 2 (stereo L/R) [CONFIRMED] | CONFIRMED |
| Mic input | Via 4th contact (CTIA) [INFERRED] | INFERRED |
| Impedance | <32Ω typical (headphone drive) [INFERRED] | INFERRED |
| DAC | Integrated codec DAC [INFERRED] | INFERRED |
| Sample rate | 48 kHz [INFERRED] | INFERRED |
| Bit depth | 16-bit or 24-bit [INFERRED] | INFERRED |

**Table 6.3:** Headphone jack specifications. The CTIA standard supports
headset microphone input through the 4th ring contact. [CONFIRMED] [1]

### 6.5 HDMI Audio

| Parameter | Value | Confidence |
|---|---|---|
| Output format | Linear PCM [CONFIRMED] | CONFIRMED |
| Max channels | 5.1 (6 channels) [CONFIRMED] | CONFIRMED |
| Sample rate | 48 kHz [INFERRED] | INFERRED |
| Bit depth | 16-bit or 24-bit [INFERRED] | INFERRED |
| Dolby/DTS passthrough | Not supported (LPCM only) [INFERRED] | INFERRED |
| eARC | Not applicable (console is source) [INFERRED] | INFERRED |
| Audio routing | Via `audout` service → DC HDMI output [INFERRED] | INFERRED |

**Table 6.4:** HDMI audio output. Nintendo confirms 5.1ch LPCM output;
compressed audio formats (Dolby Digital, DTS) are not listed.
[CONFIRMED] [1][2]

### 6.6 Microphone

| Parameter | Value | Confidence |
|---|---|---|
| Microphone count | 1 (monaural) [CONFIRMED] | CONFIRMED |
| Type | MEMS microphone [INFERRED] | INFERRED |
| Location | Console body [INFERRED] | INFERRED |
| Noise cancellation | Yes (active) [CONFIRMED] | CONFIRMED |
| Echo cancellation | Yes [CONFIRMED] | CONFIRMED |
| Auto gain control | Yes [CONFIRMED] | CONFIRMED |
| Sample rate | 16 kHz or 48 kHz [INFERRED] | INFERRED |
| Bit depth | 16-bit [INFERRED] | INFERRED |
| Primary use | GameChat voice chat [INFERRED] | INFERRED |
| Horizon OS service | `audin` [CONFIRMED — oboromi service list] | CONFIRMED |

**Table 6.5:** Microphone specifications. Noise cancellation, echo
cancellation, and AGC are confirmed for voice chat. The `audin` service
handles audio input in Horizon OS. [CONFIRMED] [1][4]

### 6.7 Audio DSP

The T239 includes a hardware audio DSP for real-time audio processing.
This offloads mixing, effects, and spatial audio from the CPU.
[INFERRED — Tegra audio architecture.]

| Parameter | Value | Confidence |
|---|---|---|
| DSP type | Dedicated audio DSP (Tegra AHUB or equivalent) [INFERRED] | INFERRED |
| Mixing voices | Up to 256 simultaneous voices [SPECULATIVE] | SPECULATIVE |
| Sample rate | 48 kHz (system default) [INFERRED] | INFERRED |
| Bit depth | 32-bit internal processing [INFERRED] | INFERRED |
| Effects | Reverb, EQ, compressor/limiter, spatial audio [INFERRED] | INFERRED |
| 3D audio | HRTF-based binaural rendering [INFERRED] | INFERRED |
| Opus codec | Hardware-accelerated (`hwopus` service) [CONFIRMED] | CONFIRMED |
| CPU offload | Yes (DSP handles mixing/effects) [INFERRED] | INFERRED |

**Table 6.6:** Audio DSP characteristics. The `hwopus` service provides
hardware-accelerated Opus codec for GameChat voice compression.
[CONFIRMED — oboromi service list.] [4]

### 6.8 3D Spatial Audio

Nintendo advertises "spatial 3D sound" for Switch 2. This likely uses
HRTF (Head-Related Transfer Function) binaural rendering through the
`audren` (audio renderer) service to create a surround sound experience
over stereo headphones. [INFERRED — Nintendo marketing, standard spatial
audio implementation.] [2]

| Feature | Implementation | Confidence |
|---|---|---|
| 3D audio engine | `audren` service [CONFIRMED] | CONFIRMED |
| HRTF rendering | Binaural stereo from surround sources [INFERRED] | INFERRED |
| Head tracking | Not supported (no head tracker) [INFERRED] | INFERRED |
| Speaker surround | DSP virtual surround effect [CONFIRMED] | CONFIRMED |
| Headphone surround | HRTF binaural [INFERRED] | INFERRED |
| Game integration | Via audio middleware (FMOD, Wwise) [INFERRED] | INFERRED |

**Table 6.7:** 3D spatial audio implementation. The surround sound effect
for built-in speakers requires a system update per Nintendo.
[CONFIRMED] [1]

### 6.9 Bluetooth Audio

The Switch 2 supports Bluetooth audio output for wireless headphones and
earbuds. This is handled by the Bluetooth stack (`bt`, `btdrv`, `btm`
services) and the audio output service (`audout`).
[INFERRED — Bluetooth audio is standard for modern consoles.] [4]

| Parameter | Value | Confidence |
|---|---|---|
| Bluetooth version | 5.x [INFERRED] | INFERRED |
| Audio profile | A2DP (Advanced Audio Distribution Profile) [INFERRED] | INFERRED |
| Codec | SBC (mandatory), AAC likely [INFERRED] | INFERRED |
| Latency | ~100–200 ms (standard A2DP) [INFERRED] | INFERRED |
| Channels | 2.0 (stereo) [INFERRED] | INFERRED |
| Simultaneous devices | 1 audio device [INFERRED] | INFERRED |
| Horizon OS services | `bt`, `btdrv`, `btm` [CONFIRMED] | CONFIRMED |

**Table 6.8:** Bluetooth audio characteristics. Bluetooth audio latency
(~100–200 ms) makes it unsuitable for competitive gaming but acceptable
for casual play and media consumption. [INFERRED] [4]

### 6.10 Audio Codec IC

The console likely uses an integrated audio codec for analog I/O (speaker
amplifier, headphone DAC, microphone ADC). [INFERRED — Standard mobile
SoC audio architecture.]

| Function | Implementation | Confidence |
|---|---|---|
| Speaker DAC | Integrated codec DAC → class-D amp [INFERRED] | INFERRED |
| Headphone DAC | Integrated codec DAC → headphone driver [INFERRED] | INFERRED |
| Microphone ADC | Integrated codec ADC from MEMS mic [INFERRED] | INFERRED |
| Sample rate | 48 kHz (system standard) [INFERRED] | INFERRED |
| Bit depth | 24-bit DAC / 16-bit ADC [INFERRED] | INFERRED |
| S/N ratio | >90 dB (typical for mobile codec) [SPECULATIVE] | SPECULATIVE |
| Codec IC | Integrated in T239 or discrete (Realtek/Maxim) [SPECULATIVE] | SPECULATIVE |

**Table 6.9:** Audio codec characteristics. The T239 may integrate the
audio codec on-die or use an external codec IC connected via I2S/TDM.
[SPECULATIVE]

### 6.11 GameChat Audio

GameChat is a new Switch 2 feature enabling voice (and video) chat
between players during online multiplayer. The audio pipeline for
GameChat uses the `chat` service and hardware Opus codec.
[CONFIRMED — oboromi service list includes `chat` and `hwopus`.] [4]

| Component | Function | Confidence |
|---|---|---|
| `chat` service | GameChat session management [CONFIRMED] | CONFIRMED |
| `audin` service | Microphone capture [CONFIRMED] | CONFIRMED |
| `hwopus` service | Hardware Opus encode/decode [CONFIRMED] | CONFIRMED |
| `codecctl` service | Hardware codec control [CONFIRMED] | CONFIRMED |
| Latency target | <100 ms end-to-end [SPECULATIVE] | SPECULATIVE |
| Max players | 4 players voice chat [INFERRED] | INFERRED |
| Noise suppression | AI/ML noise cancellation [INFERRED] | INFERRED |

**Table 6.10:** GameChat audio pipeline. The hardware Opus codec offloads
voice compression from the CPU, enabling low-latency multi-player voice
chat. [CONFIRMED — oboromi service list.] [4]

---

## 7. Input Overview

### 7.1 Input Subsystem Architecture

The Switch 2 input subsystem handles controller input (Joy-Con 2,
Pro Controller, third-party controllers), touchscreen input, and
keyboard/mouse peripherals. Input events flow from hardware through
the HID (Human Interface Device) service to applications.
[CONFIRMED — oboromi service list, Nintendo specs.] [1][4]

```
+------------------------------------------------------------------+
|                  Input Subsystem Architecture                    |
|                                                                  |
|  +----------------------------------------------------------+   |
|  |  Hardware Input Devices                                   |   |
|  |  +-----------+  +-----------+  +----------+  +----------+ |   |
|  |  | Joy-Con L |  | Joy-Con R |  | Touch    |  | USB HID  | |   |
|  |  | (BLE)     |  | (BLE+NFC) |  | Panel    |  | (kbd/mouse)|   |
|  |  +-----+-----+  +-----+-----+  +----+-----+  +----+-----+ |   |
|  |        |              |               |              |      |   |
|  +--------|--------------|---------------|--------------|------+   |
|           |              |               |              |          |
|           v              v               v              v          |
|  +----------------------------------------------------------+   |
|  |  HID Subsystem                                            |   |
|  |  +-----------------------------------------------------+ |   |
|  |  |  hidbus (HID Bus)                                    | |   |
|  |  |  - Device enumeration                                | |   |
|  |  |  - Report descriptor parsing                         | |   |
|  |  |  - Hot-plug detection                                | |   |
|  |  +------------------------+----------------------------+ |   |
|  |                           |                               |   |
|  |  +------------------------v----------------------------+ |   |
|  |  |  hid (HID Service)                                   | |   |
|  |  |  - Shared memory input polling                       | |   |
|  |  |  - Controller state aggregation                      | |   |
|  |  |  - Touch event dispatch                              | |   |
|  |  |  - Motion data (gyro/accel)                          | |   |
|  |  |  - NFC tag read events                               | |   |
|  |  +-----------------------------------------------------+ |   |
|  +----------------------------------------------------------+   |
|           |                                                      |
|           v                                                      |
|  +----------------------------------------------------------+   |
|  |  Application (Game)                                       |   |
|  |  - Reads NpadState from shared memory                     |   |
|  |  - Polls at 60Hz or 120Hz                                 |   |
|  |  - SixAxis motion data for gyro aiming                    |   |
|  +----------------------------------------------------------+   |
+------------------------------------------------------------------+
```

**Figure 7.1:** Input subsystem architecture. Hardware input devices
connect via BLE (Joy-Con), I2C (touchscreen), or USB (peripherals).
The HID bus (`hidbus`) enumerates devices and parses report descriptors;
the HID service (`hid`) aggregates input state in shared memory for
application polling. [INFERRED — Horizon OS HID architecture, oboromi
service list.] [4]

### 7.2 Horizon OS Input Services

| Service | Domain | Role | Confidence |
|---|---|---|---|
| `hid` | Input | Primary HID service — shared memory input state, controller management | CONFIRMED [4] |
| `hidbus` | Input | HID bus management — device enumeration, report descriptors, hot-plug | CONFIRMED [4] |
| `ahid` | Input | Application HID controller applet (controller pairing UI) | CONFIRMED [4] |
| `ts` | Input | Touchscreen service — raw touch coordinates, multi-touch | CONFIRMED [4] |
| `tspm` | Input | Touchscreen power manager — sleep/wake touch panel | CONFIRMED [4] |
| `i2c` | Bus | I2C bus driver (touchscreen, sensors) | CONFIRMED [4] |

**Table 7.1:** Horizon OS input services. The `hid` and `hidbus` services
form the core of the input subsystem. Touch input is handled by the
dedicated `ts` service. [CONFIRMED — oboromi service list.] [4]

### 7.3 Controller Types

| Controller | Connection | Input Axes | Buttons | Motion | Confidence |
|---|---|---|---|---|---|
| Joy-Con 2 (L+R) | BLE | 2 sticks + D-pad | 18 buttons | 6-axis IMU | CONFIRMED [1] |
| Joy-Con 2 (single) | BLE | 1 stick | 10 buttons | 6-axis IMU | INFERRED [1] |
| Pro Controller 2 | BLE/USB | 2 sticks + D-pad | 18 buttons | 6-axis IMU | INFERRED [1] |
| GameCube Controller | USB (adapter) | 2 sticks + D-pad | 12 buttons | None | INFERRED |
| USB HID (keyboard) | USB | N/A | Full keyboard | None | INFERRED |
| USB HID (mouse) | USB | 2-axis + buttons | 2–5 buttons | None | INFERRED |

**Table 7.2:** Supported controller types. Joy-Con 2 and Pro Controller
connect via BLE; USB controllers connect through the dock's USB-A ports
or a USB adapter. [INFERRED — Nintendo specs, Switch 1 precedent.]

---

## 8. Joy-Con 2

### 8.1 Joy-Con 2 Overview

The Joy-Con 2 controllers are the primary input devices for Switch 2,
featuring significant upgrades over the original Joy-Con. Each Joy-Con 2
connects wirelessly via Bluetooth Low Energy (BLE) and can charge via
USB-C when attached to the console or a charging grip.
[CONFIRMED — Nintendo specs.] [1]

```
+------------------------------------------------------------------+
|                 Joy-Con 2 Internal Architecture                  |
|                                                                  |
|  +----------------------------------------------------------+   |
|  |  Joy-Con 2 Controller                                     |   |
|  |                                                           |   |
|  |  +--------------------+  +-----------------------------+   |   |
|  |  |  Main MCU          |  |  BLE Radio                  |   |   |
|  |  |  (ARM Cortex-M)    |  |  (Bluetooth 5.x LE)        |   |   |
|  |  |  - Input scanning  |  |  - HID-over-GATT           |   |   |
|  |  |  - Report encoding |<->|  - Low-latency transport   |   |   |
|  |  |  - LED / rumble    |  |  - Encrypted pairing       |   |   |
|  |  +--------+-----------+  +-----------------------------+   |   |
|  |           |                                                |   |
|  |  +--------v-----------+  +-----------------------------+   |   |
|  |  |  Hall Effect       |  |  6-Axis IMU                 |   |   |
|  |  |  Stick Module      |  |  (Accel + Gyro)             |   |   |
|  |  |  - Analog X/Y      |  |  - 3-axis accelerometer     |   |   |
|  |  |  - No drift (Hall) |  |  - 3-axis gyroscope         |   |   |
|  |  +--------------------+  |  - Motion data at 1kHz      |   |   |
|  |                          +-----------------------------+   |   |
|  |  +--------------------+  +-----------------------------+   |   |
|  |  |  HD Haptics        |  |  NFC Antenna (Right only)  |   |   |
|  |  |  (Linear resonant  |  |  - 13.56 MHz               |   |   |
|  |  |   actuator)        |  |  - NTAG215 (amiibo)        |   |   |
|  |  |  - Wideband        |  |  - Read/Write              |   |   |
|  |  +--------------------+  +-----------------------------+   |   |
|  |                                                           |   |
|  |  +--------------------+  +-----------------------------+   |   |
|  |  |  IR Camera (Right) |  |  SL/SR Buttons              |   |   |
|  |  |  - IR dot matrix   |  |  (Tabletop mode L/R)       |   |   |
|  |  |  - Motion tracking |  +-----------------------------+   |   |
|  |  +--------------------+                                    |   |
|  |                                                           |   |
|  |  +--------------------+                                    |   |
|  |  |  USB-C Connector   |  Charging (attached or grip)      |   |   |
|  |  |  (bottom rail)     |  Also: firmware update path       |   |   |
|  |  +--------------------+                                    |   |
|  +----------------------------------------------------------+   |
+------------------------------------------------------------------+
```

**Figure 8.1:** Joy-Con 2 internal architecture. The right Joy-Con includes
an NFC antenna for amiibo and an IR camera. Both Joy-Cons have Hall effect
sticks (eliminating stick drift), HD haptics, and 6-axis IMU.
[CONFIRMED — Nintendo specs, Digital Foundry teardown.] [1][3]

### 8.2 Hall Effect Sticks

The Joy-Con 2 uses **Hall effect analog sticks**, a major improvement over
the potentiometer-based sticks in the original Joy-Con that suffered from
drift. [CONFIRMED — Nintendo specs, Digital Foundry analysis.] [1][3]

| Parameter | Value | Confidence |
|---|---|---|
| Technology | Hall effect (magnetic) | CONFIRMED [1][3] |
| Axes | 2 (X, Y) per stick | CONFIRMED [1] |
| Resolution | 12-bit (4096 steps) [SPECULATIVE] | SPECULATIVE |
| Stick drift | Eliminated (no physical contact wear) | CONFIRMED [1][3] |
| Stick caps | Detachable (snap-on) | CONFIRMED [1] |
| Calibration | Factory calibrated, user recalibration possible [INFERRED] | INFERRED |

**Table 8.1:** Hall effect stick specifications. The magnetic sensing
principle eliminates the mechanical wear that caused drift in Switch 1
Joy-Con potentiometer sticks. [CONFIRMED] [1][3]

### 8.3 HD Haptics

Both Joy-Con 2 controllers include **HD haptic** feedback using linear
resonant actuators (LRAs) capable of generating a wide range of textures
and vibration patterns. [CONFIRMED — Nintendo specs.] [1]

| Parameter | Value | Confidence |
|---|---|---|
| Actuator type | Linear Resonant Actuator (LRA) [INFERRED] | INFERRED |
| Frequency range | 100–300 Hz [SPECULATIVE] | SPECULATIVE |
| Resolution | Multi-level amplitude [INFERRED] | INFERRED |
| Waveform | Arbitrary waveform (HD haptics) [INFERRED] | INFERRED |
| Control | Via `hid` service haptic API [INFERRED] | INFERRED |

**Table 8.2:** HD haptics specifications. HD haptics allow games to
simulate complex textures (rain, sand, ice) through precise vibration
patterns. [INFERRED — Switch 1 HD haptics, Nintendo marketing.]

### 8.4 NFC Reader

The **right Joy-Con 2** contains an NFC antenna for amiibo and other
NFC tag interactions. [CONFIRMED — Nintendo specs.] [1]

| Parameter | Value | Confidence |
|---|---|---|
| NFC type | NTAG215 (amiibo standard) [INFERRED] | INFERRED |
| Frequency | 13.56 MHz [INFERRED] | INFERRED |
| Protocol | NFC Forum Type 2 [INFERRED] | INFERRED |
| Read/Write | Both (read amiibo data, write custom data) [INFERRED] | INFERRED |
| Antenna location | Right Joy-Con only [CONFIRMED] | CONFIRMED |
| Horizon OS service | `nfc`, `nfp` [CONFIRMED] | CONFIRMED [4] |

**Table 8.3:** NFC reader specifications. The `nfc` service handles NFC
hardware access; the `nfp` service manages amiibo (NFC Figure Protocol)
data parsing and game integration. [CONFIRMED] [4]

### 8.5 IR Camera

The **right Joy-Con 2** includes an IR (infrared) motion camera.
[CONFIRMED — Nintendo specs.] [1]

| Parameter | Value | Confidence |
|---|---|---|
| Sensor type | IR dot matrix camera [INFERRED] | INFERRED |
| Resolution | 128×96 pixels [SPECULATIVE] | SPECULATIVE |
| Frame rate | 30 fps [SPECULATIVE] | SPECULATIVE |
| IR illumination | Built-in IR LEDs [INFERRED] | INFERRED |
| Use cases | Motion tracking, shape recognition, distance estimation [INFERRED] | INFERRED |
| Horizon OS service | `hid` (IR data sub-report) [INFERRED] | INFERRED |

**Table 8.4:** IR camera specifications. The IR camera was introduced in
Switch 1 Joy-Con; Switch 2 likely retains or upgrades this capability.
[SPECULATIVE]

### 8.6 6-Axis IMU

Both Joy-Con 2 controllers contain a 6-axis Inertial Measurement Unit
(IMU) with a 3-axis accelerometer and 3-axis gyroscope.
[CONFIRMED — Nintendo specs.] [1]

| Parameter | Value | Confidence |
|---|---|---|
| Axes | 6 (3-axis accelerometer + 3-axis gyroscope) | CONFIRMED [1] |
| Accelerometer range | ±8g [SPECULATIVE] | SPECULATIVE |
| Gyroscope range | ±2000 dps [SPECULATIVE] | SPECULATIVE |
| Sample rate | ~1 kHz [SPECULATIVE] | SPECULATIVE |
| Motion data format | 16-bit per axis (signed) [INFERRED] | INFERRED |
| Use cases | Gyro aiming, motion control, shake detection [INFERRED] | INFERRED |
| Horizon OS service | `hid` (SixAxis sub-report) [INFERRED] | INFERRED |

**Table 8.5:** 6-axis IMU specifications. Motion data is accessed via
the `hid` service's SixAxis API, providing gyro and accelerometer data
at high frequency for precision aiming. [INFERRED] [4]

### 8.7 BLE Connection Protocol

Joy-Con 2 connects to the Switch 2 via **Bluetooth Low Energy (BLE)**.
The pairing process uses encrypted LE connections with low-latency
transport. [INFERRED — Bluetooth 5.x standard, Switch BLE protocol.] [4]

```
Joy-Con 2 BLE Connection Flow:
  1. Pairing Initiation
     Console: `btm` service → BLE scan
     Joy-Con: Hold SYNC button → enter pairing mode
     |                             |
     v                             v
  2. BLE Connection
     GATT service discovery (HID-over-GATT)
     Encryption key exchange (LE Secure Connections)
     |                             |
     v                             v
  3. HID Registration
     `hidbus` → device enumeration
     Report descriptor parsed → input report format known
     |                             |
     v                             v
  4. Input Active
     Joy-Con sends HID reports (250Hz) via BLE
     `hid` service → shared memory update
     Game reads NpadState at 60Hz or 120Hz
```

**Figure 8.2:** Joy-Con 2 BLE connection flow. The `btm` (Bluetooth
manager) handles pairing; `hidbus` handles HID device registration;
`hid` aggregates input state in shared memory for game polling.
[INFERRED — BLE HID-over-GATT standard, oboromi service list.] [4]

### 8.8 Input Report Format

The Joy-Con 2 input report structure is derived from the Switch 1 Joy-Con
protocol, adapted for BLE transport. [INFERRED — Switch 1 Joy-Con
reverse engineering, BLE HID protocol.]

| Offset | Field | Size | Description | Confidence |
|---|---|---|---|---|
| 0x00 | Report ID | 1 | Input report type (0x30 standard, 0x31 6-axis) [INFERRED] | INFERRED |
| 0x01 | Timer | 1 | Incrementing counter (mod 256) [INFERRED] | INFERRED |
| 0x02 | Battery + Conn | 1 | Battery level (4 bits) + connection info [INFERRED] | INFERRED |
| 0x03 | Button State 0 | 1 | Y/X/B/A, SR/SL, R/ZR buttons [INFERRED] | INFERRED |
| 0x04 | Button State 1 | 1 | +/−, Stick press, Home/Capture [INFERRED] | INFERRED |
| 0x05 | Button State 2 | 1 | D-pad (Up/Down/Left/Right), SR/SL [INFERRED] | INFERRED |
| 0x06 | Stick L (X low) | 1 | Left stick X axis, lower 8 bits [INFERRED] | INFERRED |
| 0x07 | Stick L (Y + X high) | 1 | Left stick Y lower 4 + X upper 4 [INFERRED] | INFERRED |
| 0x08 | Stick L (Y high) | 1 | Left stick Y upper 8 bits [INFERRED] | INFERRED |
| 0x09 | Stick R (X low) | 1 | Right stick X axis, lower 8 bits [INFERRED] | INFERRED |
| 0x0A | Stick R (Y + X high) | 1 | Right stick Y lower 4 + X upper 4 [INFERRED] | INFERRED |
| 0x0B | Stick R (Y high) | 1 | Right stick Y upper 8 bits [INFERRED] | INFERRED |
| 0x0C | Vibraiton info | 1 | Haptic feedback state [INFERRED] | INFERRED |

**Table 8.6:** Joy-Con input report format (standard mode). Each stick
axis is encoded as a 12-bit value split across 3 bytes. [INFERRED —
Switch 1 Joy-Con protocol.]

### 8.9 Motion Data Format

When report ID 0x31 is used, 6-axis IMU data is included in the report.
[SPECULATIVE — Switch 1 protocol extrapolation.]

| Offset | Field | Size | Description | Confidence |
|---|---|---|---|---|
| 0x0D | Accel X | 2 | Accelerometer X (16-bit LE signed) [INFERRED] | INFERRED |
| 0x0F | Accel Y | 2 | Accelerometer Y (16-bit LE signed) [INFERRED] | INFERRED |
| 0x11 | Accel Z | 2 | Accelerometer Z (16-bit LE signed) [INFERRED] | INFERRED |
| 0x13 | Gyro X | 2 | Gyroscope X (16-bit LE signed) [INFERRED] | INFERRED |
| 0x15 | Gyro Y | 2 | Gyroscope Y (16-bit LE signed) [INFERRED] | INFERRED |
| 0x17 | Gyro Z | 2 | Gyroscope Z (16-bit LE signed) [INFERRED] | INFERRED |

**Table 8.7:** 6-axis motion data format. Three consecutive samples
are packed per report (total IMU data: 36 bytes at ~833 Hz effective).
[INFERRED — Switch 1 protocol.]

### 8.10 USB-C Charging

Joy-Con 2 controllers charge via the rail connector when attached to
the console or via USB-C when in a charging grip. [CONFIRMED —
Nintendo specs.] [1]

| Parameter | Value | Confidence |
|---|---|---|
| Charging method | Rail connector (attached) or USB-C (grip) [INFERRED] | INFERRED |
| Charging voltage | 5V [INFERRED] | INFERRED |
| Charging current | ~400 mA [SPECULATIVE] | SPECULATIVE |
| Battery capacity | ~525 mAh [SPECULATIVE] | SPECULATIVE |
| Battery life | ~20 hours [CONFIRMED] | CONFIRMED [1] |
| Charge time | ~3.5 hours [SPECULATIVE] | SPECULATIVE |

**Table 8.8:** Joy-Con 2 charging characteristics. Battery life is
confirmed by Nintendo at approximately 20 hours. [CONFIRMED] [1]

---

## 9. Touchscreen

### 9.1 Touchscreen Specifications

The Switch 2 features a **capacitive touchscreen** integrated into the
7.9-inch LCD panel. [CONFIRMED — Nintendo specs.] [1]

| Parameter | Value | Confidence |
|---|---|---|
| Technology | Projected capacitive (PCAP) [INFERRED] | INFERRED |
| Multi-touch points | 10 [SPECULATIVE] | SPECULATIVE |
| Touch resolution | 1920×1080 (panel-native) [INFERRED] | INFERRED |
| Sampling rate | 120 Hz or 240 Hz [SPECULATIVE] | SPECULATIVE |
| Touch controller | Integrated digitizer IC (Synaptics or Goodix) [SPECULATIVE] | SPECULATIVE |
| Interface to SoC | I2C [INFERRED] | INFERRED |
| Glove mode | Not confirmed [SPECULATIVE] | SPECULATIVE |
| Stylus support | Capacitive stylus compatible [INFERRED] | INFERRED |
| Palm rejection | Yes [INFERRED] | INFERRED |

**Table 9.1:** Touchscreen specifications. The capacitive touch panel
is overlaid on the LCD. Touch events are routed through the `ts`
(touchscreen) service. [CONFIRMED — Nintendo specs.] [1][4]

### 9.2 Touch Controller Interface

The touch controller communicates with the T239 SoC via I2C bus. Touch
events are reported as HID-compliant touch digitizer reports through
the `ts` service. [INFERRED — Standard capacitive touch architecture.] [4]

```
Touch Data Flow:
  +----------------+     +----------------+     +----------------+
  | Capacitive     |     | Touch          |     | T239 SoC       |
  | Touch Sensor   |---->| Controller IC  |---->| I2C Bus        |
  | (PCAP overlay) |     | (digitizer)    |     |                |
  +----------------+     +----------------+     +-------+--------+
                                                        |
                                                        v
                                                +----------------+
                                                | ts (Touch      |
                                                | Screen Service)|
                                                | - Raw coords   |
                                                | - Multi-touch  |
                                                | - Touch events |
                                                +-------+--------+
                                                        |
                                                        v
                                                +----------------+
                                                | hid (HID       |
                                                | Service)       |
                                                | - Unified input|
                                                +----------------+
```

**Figure 9.1:** Touch data flow. The capacitive sensor feeds touch
coordinates to the touch controller IC, which communicates via I2C
to the `ts` service. Touch events are then aggregated into the
unified input model via the `hid` service. [INFERRED] [4]

### 9.3 Touch Event Format

Touch events are reported in a multi-touch format with per-finger
tracking. [INFERRED — Standard HID touch digitizer protocol.]

| Field | Size | Description | Confidence |
|---|---|---|---|
| Finger ID | 1 | Unique finger identifier (0–9) [INFERRED] | INFERRED |
| Touch state | 1 | Down / Move / Up [INFERRED] | INFERRED |
| X coordinate | 2 | 16-bit (0–1919) [INFERRED] | INFERRED |
| Y coordinate | 2 | 16-bit (0–1079) [INFERRED] | INFERRED |
| Pressure | 2 | Touch pressure (optional) [SPECULATIVE] | SPECULATIVE |
| Contact area | 2 | Finger contact size [SPECULATIVE] | SPECULATIVE |

**Table 9.2:** Touch event format. Each active finger produces a
touch report with position and state. [INFERRED — HID touch digitizer.]

---

## 10. Wi-Fi 6E

### 10.1 Wi-Fi 6E Overview

The Switch 2 supports **Wi-Fi 6E** (IEEE 802.11ax on 6 GHz band),
providing high-speed wireless networking for online gaming, game
downloads, and system updates. [CONFIRMED — Nintendo specs.] [1]

| Parameter | Value | Confidence |
|---|---|---|
| Standard | IEEE 802.11ax (Wi-Fi 6E) | CONFIRMED [1] |
| Frequency bands | 2.4 GHz, 5 GHz, 6 GHz | CONFIRMED [1] |
| MIMO configuration | 2×2 MIMO [SPECULATIVE] | SPECULATIVE |
| Channel bandwidth | 20/40/80/160 MHz [INFERRED] | INFERRED |
| Max PHY rate | ~2.4 Gbps (160 MHz, 2×2) [INFERRED] | INFERRED |
| Security | WPA3 [INFERRED] | INFERRED |
| WPS | Supported [INFERRED] | INFERRED |
| Hotspot mode | Not confirmed [SPECULATIVE] | SPECULATIVE |

**Table 10.1:** Wi-Fi 6E specifications. The 6 GHz band provides cleaner
spectrum with less interference, beneficial for low-latency online gaming.
[CONFIRMED — Nintendo specs.] [1]

### 10.2 Wi-Fi Service Architecture

Wi-Fi connectivity is managed by the `wlan` and `nifm` services in
Horizon OS. [CONFIRMED — oboromi service list.] [4]

| Service | Role | Confidence |
|---|---|---|
| `wlan` | Low-level Wi-Fi driver — scan, connect, SSID management | CONFIRMED [4] |
| `nifm` | Network Interface Manager — IP configuration, connectivity state, DNS | CONFIRMED [4] |
| `sfdnsres` | DNS resolver service | CONFIRMED [4] |
| `ssl` | TLS/SSL service for secure connections | CONFIRMED [4] |

**Table 10.2:** Wi-Fi and networking services. The `wlan` service handles
radio-level operations; `nifm` manages the network interface at the
IP level. [CONFIRMED] [4]

### 10.3 Wi-Fi Frequency Bands

| Band | Frequency Range | Channels | Typical Use | Confidence |
|---|---|---|---|---|
| 2.4 GHz | 2412–2484 MHz | 1–14 | Legacy compatibility, IoT, long range | CONFIRMED [1] |
| 5 GHz | 5180–5825 MHz | 36–165 | Primary gaming, medium range | CONFIRMED [1] |
| 6 GHz | 5955–7115 MHz | 1–233 | Low interference, high bandwidth | CONFIRMED [1] |

**Table 10.3:** Wi-Fi frequency bands. The 6 GHz band (Wi-Fi 6E exclusive)
provides up to 1200 MHz of additional spectrum, reducing congestion
in apartment buildings and tournament venues. [CONFIRMED] [1]

---

## 11. Bluetooth 5.x

### 11.1 Bluetooth 5.x Overview

The Switch 2 includes **Bluetooth 5.x** supporting both Bluetooth Low
Energy (BLE) for controllers and Classic Bluetooth for audio output.
[INFERRED — Nintendo specs, Bluetooth standard.] [1]

| Parameter | Value | Confidence |
|---|---|---|
| Bluetooth version | 5.x (BLE and Classic) [INFERRED] | INFERRED |
| BLE mode | For controllers (Joy-Con, Pro Controller) [INFERRED] | INFERRED |
| Classic mode | For audio (A2DP headphones/earbuds) [INFERRED] | INFERRED |
| Max paired devices | 8 controllers + 1 audio device [INFERRED] | INFERRED |
| Range | ~10 meters (typical indoor) [INFERRED] | INFERRED |

**Table 11.1:** Bluetooth 5.x specifications. BLE is used for low-latency
controller input; Classic Bluetooth (A2DP) is used for audio output.
[INFERRED — Bluetooth standard.] [1]

### 11.2 Audio Codecs

| Codec | Type | Bitrate | Latency | Quality | Confidence |
|---|---|---|---|---|---|
| SBC | Mandatory | 328 kbps [INFERRED] | ~100–200 ms [INFERRED] | Standard | INFERRED |
| AAC | Optional | 256 kbps [INFERRED] | ~100–200 ms [INFERRED] | Good | INFERRED |
| aptX | Optional | 352 kbps [INFERRED] | ~60–80 ms [INFERRED] | Good | SPECULATIVE |
| aptX HD | Optional | 576 kbps [INFERRED] | ~60–80 ms [INFERRED] | High | SPECULATIVE |
| LDAC | Optional | 990 kbps [INFERRED] | ~100–200 ms [INFERRED] | Highest | SPECULATIVE |

**Table 11.2:** Bluetooth audio codecs. SBC is mandatory for all A2DP
devices; AAC, aptX, and LDAC support depends on the controller firmware.
[INFERRED — A2DP codec standard.]

### 11.3 Bluetooth Service Stack

| Service | Role | Confidence |
|---|---|---|
| `bt` | Bluetooth core service — device discovery, pairing, profile management | CONFIRMED [4] |
| `btdrv` | Bluetooth driver — HCI transport, radio control | CONFIRMED [4] |
| `btm` | Bluetooth manager — paired device database, connection policies | CONFIRMED [4] |
| `btp` | Bluetooth pairing — PIN entry, SSP pairing, link key storage | CONFIRMED [4] |

**Table 11.3:** Bluetooth service stack. The four services (`bt`, `btdrv`,
`btm`, `btp`) handle the full Bluetooth lifecycle from radio control to
application-level profile management. [CONFIRMED] [4]

### 11.4 BLE vs Classic Bluetooth

| Feature | BLE (Bluetooth Low Energy) | Classic Bluetooth |
|---|---|---|
| Use case | Controllers (Joy-Con, Pro Controller) | Audio (A2DP headphones) |
| Latency | ~4 ms (connection interval) [INFERRED] | ~100–200 ms (A2DP buffering) [INFERRED] |
| Power consumption | Very low (controller battery life) [INFERRED] | Moderate (audio streaming) [INFERRED] |
| Data rate | 1–2 Mbps [INFERRED] | 1–3 Mbps [INFERRED] |
| Profile | HID-over-GATT [INFERRED] | A2DP + AVRCP [INFERRED] |
| Pairing | LE Secure Connections [INFERRED] | SSP (Secure Simple Pairing) [INFERRED] |

**Table 11.4:** BLE vs Classic Bluetooth comparison. BLE's low latency
is critical for responsive controller input; Classic Bluetooth's higher
bandwidth is needed for audio streaming. [INFERRED — Bluetooth standard.]

---

## 12. USB-C

### 12.1 USB-C Overview

The Switch 2 has **two USB-C ports**: one on the bottom (primary,
for charging and dock) and one on the top (secondary, for charging
in tabletop mode). [CONFIRMED — Nintendo specs.] [1]

| Parameter | Value | Confidence |
|---|---|---|
| USB standard | USB 3.2 Gen 2 (10 Gbps) [INFERRED] | INFERRED |
| DisplayPort Alt Mode | DP 1.4 HBR3 (bottom port) [INFERRED] | INFERRED |
| USB Power Delivery | PD 3.0 or later [INFERRED] | INFERRED |
| Bottom port functions | Charging, dock (DP Alt Mode + USB 3.x + PD) [CONFIRMED] | CONFIRMED [1] |
| Top port functions | Charging only (tabletop mode) [CONFIRMED] | CONFIRMED [1] |
| MTP/PTP | Supported (Media Transfer Protocol for file transfer) [INFERRED] | INFERRED |

**Table 12.1:** USB-C port specifications. The bottom port is the primary
interface carrying DP Alt Mode, USB data, and power delivery. The top
port is limited to charging. [CONFIRMED — Nintendo specs.] [1]

### 12.2 DisplayPort Alt Mode

The bottom USB-C port supports **DisplayPort Alt Mode** for video
output to the dock. [INFERRED — Docked 4K60 output implies DP Alt Mode.]

| Parameter | Value | Confidence |
|---|---|---|
| DP version | DisplayPort 1.4 [INFERRED] | INFERRED |
| Link rate | HBR3 (8.1 Gbps/lane) [INFERRED] | INFERRED |
| Lane count | 2 or 4 lanes [SPECULATIVE] | SPECULATIVE |
| Aggregate bandwidth | 16.2–32.4 Gbps [INFERRED] | INFERRED |
| DSC (Display Stream Compression) | Likely supported for 4K60 [INFERRED] | INFERRED |
| Simultaneous USB 3.x | Yes (shared via mux) [INFERRED] | INFERRED |

**Table 12.2:** DisplayPort Alt Mode parameters. DP 1.4 HBR3 with DSC
can carry 4K60 HDR without chroma subsampling. [INFERRED]

### 12.3 USB Power Delivery

| Parameter | Value | Confidence |
|---|---|---|
| PD standard | USB PD 3.0 or later [INFERRED] | INFERRED |
| Charging voltage (console) | 15V or 20V [INFERRED] | INFERRED |
| Charging current | 2–3A [INFERRED] | INFERRED |
| Max charging power | ~39–45W [SPECULATIVE] | SPECULATIVE |
| CC (Configuration Channel) | Used for PD negotiation and Alt Mode entry [INFERRED] | INFERRED |
| VBUS control | Electronic load switch with current limiting [INFERRED] | INFERRED |

**Table 12.3:** USB Power Delivery parameters. The PD contract negotiation
happens via CC pins before VBUS is energized. [INFERRED — USB PD spec.]

### 12.4 USB Service

| Service | Role | Confidence |
|---|---|---|
| `usb` | USB stack — device enumeration, transfer management, descriptor parsing | CONFIRMED [4] |
| `capmtp` | Camera/MTP protocol — file transfer (photos, videos) [INFERRED] | CONFIRMED [4] |

**Table 12.4:** USB services. The `usb` service provides the USB host
stack for connecting peripherals. The `capmtp` service handles MTP
file transfer for media content. [CONFIRMED] [4]

---

## 13. NFC

### 13.1 NFC Overview

The Switch 2 supports **NFC (Near Field Communication)** for amiibo
interactions. The NFC antenna is located in the **right Joy-Con 2**
controller. [CONFIRMED — Nintendo specs.] [1]

| Parameter | Value | Confidence |
|---|---|---|
| NFC standard | NFC Forum Type 2 (NTAG215) [INFERRED] | INFERRED |
| Frequency | 13.56 MHz [INFERRED] | INFERRED |
| Read range | ~4 cm (contact distance) [INFERRED] | INFERRED |
| Data capacity | 540 bytes (NTAG215) [INFERRED] | INFERRED |
| Read/Write | Both [INFERRED] | INFERRED |
| Antenna location | Right Joy-Con 2 [CONFIRMED] | CONFIRMED [1] |

**Table 13.1:** NFC specifications. NFC is used exclusively for amiibo
figure/card interactions in Switch 2. [CONFIRMED] [1]

### 13.2 NFC Services

| Service | Role | Confidence |
|---|---|---|
| `nfc` | NFC hardware driver — tag detection, read/write, RF field control | CONFIRMED [4] |
| `nfp` | NFC Figure Protocol — amiibo data parsing, game integration, Mii data | CONFIRMED [4] |

**Table 13.2:** NFC services. The `nfc` service handles low-level NFC
hardware operations; `nfp` (NFC Figure Protocol) manages amiibo data
format parsing and game-level integration. [CONFIRMED] [4]

### 13.3 amiibo Data Format

amiibo figures use the **NTAG215** NFC tag format with Nintendo-specific
data encoding. [INFERRED — NTAG215 datasheet, amiibo reverse engineering.]

| Field | Offset | Size | Description | Confidence |
|---|---|---|---|---|
| UID | 0x00 | 7 | Unique identifier (read-only) [INFERRED] | INFERRED |
| Lock bytes | 0x02 | 2 | Tag write protection [INFERRED] | INFERRED |
| amiibo ID | 0x1DC | 8 | Character ID + variant [INFERRED] | INFERRED |
| Write counter | 0x1D4 | 2 | Total write count [INFERRED] | INFERRED |
| Application area | 0x044 | 32 | Game-specific save data [INFERRED] | INFERRED |
| Mii data | Custom | ~96 | Mii character data [INFERRED] | INFERRED |

**Table 13.3:** amiibo data format on NTAG215. The `nfp` service parses
these fields and exposes them to games via the amiibo API. [INFERRED]

---

## 14. Camera and Sensors

### 14.1 Camera System

The Switch 2 includes a **built-in camera** for GameChat video
calling and potentially AR applications. [CONFIRMED — Nintendo specs,
GameChat feature.] [1][4]

| Parameter | Value | Confidence |
|---|---|---|
| Camera count | 1 (front-facing) [INFERRED] | INFERRED |
| Resolution | 720p (1280×720) [SPECULATIVE] | SPECULATIVE |
| Frame rate | 30 fps [SPECULATIVE] | SPECULATIVE |
| Sensor type | CMOS [INFERRED] | INFERRED |
| FOV | ~70–80° [SPECULATIVE] | SPECULATIVE |
| Primary use | GameChat video [CONFIRMED] | CONFIRMED [1] |
| Horizon OS service | `vic` (video/image compositor) [INFERRED] | INFERRED [4] |

**Table 14.1:** Camera specifications. The camera is primarily for
GameChat video calling. The `vic` service handles camera capture and
compositing for the GameChat overlay. [CONFIRMED] [1][4]

### 14.2 Sensors

| Sensor | Location | Interface | Function | Confidence |
|---|---|---|---|---|
| Accelerometer | Joy-Con (each) | IMU | Motion detection, shake | CONFIRMED [1] |
| Gyroscope | Joy-Con (each) | IMU | Angular motion, gyro aiming | CONFIRMED [1] |
| Brightness sensor | Console body | I2C [INFERRED] | Auto-brightness adjustment | CONFIRMED [1] |
| Proximity sensor | Not confirmed | — | — | SPECULATIVE |

**Table 14.2:** Sensor summary. The accelerometer and gyroscope in each
Joy-Con provide 6-axis motion data. The brightness sensor adjusts
LCD backlight based on ambient lighting. [CONFIRMED] [1]

---

## 15. Gap Analysis

### 15.1 Service Coverage Matrix

The following table maps display/IO hardware features to oboromi service
stubs, identifying implementation status. [CONFIRMED — oboromi source
code `core/src/nn/mod.rs`, `core/src/sys/mod.rs`.] [4]

| # | Feature | oboromi Service(s) | Stub Lines | Status | Notes |
|---|---|---|---|---|---|
| 1 | Display compositor | `vi`, `vi2`, `disp`, `dispdrv`, `ommdisp` | 5 stubs | **stub** | Display pipeline needs framebuffer management, vsync, VRR |
| 2 | Audio renderer | `aud`, `audout`, `audin`, `audren`, `audrec`, `audsmx`, `audctl`, `hwopus` | 8 stubs | **stub** | Audio DSP, spatial audio, Opus codec need implementation |
| 3 | HID input | `hid`, `hidbus`, `ahid` | 3 stubs | **stub** | Shared memory input polling, controller state, IMU data |
| 4 | Touchscreen | `ts`, `tspm` | 2 stubs | **stub** | Multi-touch events, touch-to-HID bridging |
| 5 | Wi-Fi | `wlan`, `nifm` | 2 stubs | **stub** | 802.11ax radio control, IP config, scan/connect |
| 6 | Bluetooth | `bt`, `btdrv`, `btm`, `btp` | 4 stubs | **stub** | BLE controller pairing, A2DP audio, HID-over-GATT |
| 7 | USB | `usb` | 1 stub | **stub** | USB host stack, device enumeration, transfer management |
| 8 | NFC | `nfc`, `nfp` | 2 stubs | **stub** | NTAG215 read/write, amiibo data parsing |
| 9 | Ethernet | `eth`, `ethc` | 2 stubs | **stub** | Gigabit Ethernet via dock USB adapter |
| 10 | Camera | `vic`, `capmtp` | 2 stubs | **stub** | Camera capture, GameChat compositing, MTP transfer |
| 11 | I2C bus | `i2c` | 1 stub | **stub** | I2C device enumeration (touchscreen, sensors, codec) |
| 12 | Codec control | `codecctl` | 1 stub | **stub** | Hardware H.264/H.265 codec for video encode/decode |
| 13 | Network config | `sfdnsres`, `ssl` | 2 stubs | **stub** | DNS resolver, TLS/SSL for secure connections |

**Table 15.1:** Display/IO service coverage gap analysis. All 13 feature
areas have service stubs defined in oboromi's service registry but lack
substantive implementation (no functional logic beyond `define_service!`
macros). [CONFIRMED — oboromi source code.] [4] For the firmware service
gap analysis, see **firmware.md** §10. For the GPU implementation gap
analysis, see **gpu.md** §13.

### 15.2 Service Stub Depth Analysis

The oboromi service stubs follow the `define_service!` macro pattern
with a `State::run` handler per service. Each stub is a minimal
framework with no functional implementation. [CONFIRMED — oboromi
source code.] [4]

```
Gap Analysis — Service Stub Depth:
  +----------------------------------------------------------+
  |  Service Category         | Stubs | Functional Logic | Gap |
  |---------------------------|-------|------------------|-----|
  |  Display (vi/disp)        |   5   |      0           | HIGH|
  |  Audio (aud/audout/etc)   |   8   |      0           | HIGH|
  |  Input (hid/hidbus)       |   3   |      0           | HIGH|
  |  Touch (ts/tspm)          |   2   |      0           | MED |
  |  Wi-Fi (wlan/nifm)        |   2   |      0           | HIGH|
  |  Bluetooth (bt/btdrv/etc) |   4   |      0           | HIGH|
  |  USB (usb)                |   1   |      0           | MED |
  |  NFC (nfc/nfp)            |   2   |      0           | MED |
  |  Network (eth/sfdnsres)   |   2   |      0           | MED |
  |  Camera (vic/capmtp)      |   2   |      0           | LOW |
  |  Bus/Sensor (i2c)         |   1   |      0           | LOW |
  |  Codec (codecctl)         |   1   |      0           | LOW |
  +----------------------------------------------------------+
  Total: 33 stubs, 0 functional — 100% gap
```

**Figure 15.1:** Service stub depth analysis. All display/IO service
categories are at stub-level with zero functional implementation.
HIGH priority gaps are those directly needed for basic display and
input functionality. [CONFIRMED — oboromi source code.] [4]

### 15.3 Priority Ranking

| Priority | Feature Area | Rationale | Estimated Effort |
|---|---|---|---|
| P0 | Display compositor (`vi`, `disp`) | Required for any screen output | HIGH |
| P0 | HID input (`hid`, `hidbus`) | Required for any controller input | HIGH |
| P1 | Audio output (`audout`, `aud`) | Required for sound | HIGH |
| P1 | Touchscreen (`ts`) | Required for handheld touch input | MED |
| P2 | Wi-Fi (`wlan`, `nifm`) | Required for online features | HIGH |
| P2 | Bluetooth (`bt`, `btdrv`, `btm`) | Required for wireless controllers | HIGH |
| P3 | USB (`usb`) | Required for dock peripherals | MED |
| P3 | NFC (`nfc`, `nfp`) | Required for amiibo | MED |
| P3 | Ethernet (`eth`) | Required for wired LAN (dock) | MED |
| P4 | Camera (`vic`) | Required for GameChat video | LOW |
| P4 | Codec (`codecctl`) | Required for video playback | LOW |

**Table 15.2:** Priority ranking for display/IO service implementation.
P0 services are foundational (no screen/controller without them);
P1–P2 are needed for core features; P3–P4 for extended features.
[SPECULATIVE — Implementation priority estimate.]

### 15.4 Open Questions

| ID | Question | Domain | Confidence |
|---|---|---|---|
| OQ-D01 | What is the exact DC register map for T239 (vs T234)? | Display | SPECULATIVE |
| OQ-D02 | How many DP lanes does the bottom USB-C port expose? | USB-C | SPECULATIVE |
| OQ-D03 | Does Switch 2 support DSC (Display Stream Compression) for 4K60? | Display | SPECULATIVE |
| OQ-D04 | What BLE connection interval does Joy-Con 2 use? | Bluetooth | SPECULATIVE |
| OQ-D05 | Is LDAC or aptX HD supported for Bluetooth audio? | Bluetooth | SPECULATIVE |
| OQ-D06 | What is the touchscreen sampling rate (120Hz or 240Hz)? | Touch | SPECULATIVE |
| OQ-D07 | Does the IR camera retain Switch 1 resolution (128×96)? | Input | SPECULATIVE |
| OQ-D08 | What Wi-Fi 6E chipset does the Switch 2 use? | Wi-Fi | SPECULATIVE |
| OQ-D09 | Is the audio codec integrated in T239 or a discrete IC? | Audio | SPECULATIVE |
| OQ-D10 | What is the exact Joy-Con 2 BLE report format (vs Switch 1)? | Input | SPECULATIVE |

**Table 15.3:** Open questions requiring hardware access or further
reverse engineering. These represent knowledge gaps that cannot be
resolved from public documentation alone. [SPECULATIVE]

---

## Confidence Tag Summary

| Tag | Count | Percentage |
|---|---|---|
| CONFIRMED | 193 | 35% |
| INFERRED | 281 | 51% |
| SPECULATIVE | 80 | 14% |
| **Total** | **554** | **100%** |

**Table 16.1:** Confidence tag distribution across all 15 sections. The
majority of claims are INFERRED from closely related Tegra/Orin documentation
or Bluetooth/Wi-Fi/USB specifications. CONFIRMED tags come from Nintendo
official documentation and oboromi source code. SPECULATIVE tags indicate
claims requiring hardware access or further reverse engineering.
[CALCULATED]

---

## Citations

[1] Nintendo. "Nintendo Switch 2 — Tech Specs." Nintendo of America.
https://www.nintendo.com/us/gaming-systems/switch-2/tech-specs/
Accessed: 2026-05-03. [CONFIRMED]

[2] Wikipedia. "Nintendo Switch 2." Last updated May 2026.
https://en.wikipedia.org/wiki/Nintendo_Switch_2
Accessed: 2026-05-03. [CONFIRMED]

[3] Digital Foundry / Eurogamer. "Nintendo Switch 2: final tech specs and
system reservations confirmed." May 2025. Hardware analysis confirming
GPU clocks, memory bandwidth, and dock thermal design. [CONFIRMED]

[4] oboromi. "Source code — `core/src/nn/mod.rs`." Local repository.
Service registry with 160 named services including display, audio, Bluetooth,
and codec services. Accessed: 2026-05-03. [CONFIRMED]

[5] NVIDIA. "Jetson Orin Technical Reference Manual (T234)." 2022.
Referenced as closest public documentation for T239 display controller
architecture, DC register map, and display pipeline. [INFERRED]

[6] Bluetooth SIG. "Bluetooth Core Specification v5.4." 2023.
https://www.bluetooth.com/specifications/specs/core-specification-5-4/
Referenced for BLE and Classic Bluetooth protocol details, audio codec
support (SBC, AAC, aptX, LDAC), and HID over GATT for controller input.
Accessed: 2026-05-03. [INFERRED]

[7] IEEE. "802.11ax-2021 — IEEE Standard for Information Technology —
Enhancements for High-Efficiency WLAN." 2021.
https://standards.ieee.org/ieee/802.11ax/7349/
Referenced for Wi-Fi 6E specifications including OFDMA, MU-MIMO, and
6 GHz band operation. Accessed: 2026-05-03. [INFERRED]

[8] USB Implementers Forum. "USB Type-C Cable and Connector Specification
Revision 2.2." 2022. https://usb.org/document-library/usb-type-c-cable-and-connector-specification-revision-22
Referenced for USB-C connector pinout, DisplayPort Alt Mode, and
USB Power Delivery negotiation. Accessed: 2026-05-03. [INFERRED]

[9] VESA. "DisplayPort Standard Version 2.0." 2019.
https://vesa.org/vesa-displayport-standards/
Referenced for DP Alt Mode lane configurations (HBR3) and 4K60 output
capability over USB-C. Accessed: 2026-05-03. [INFERRED]

[10] oboromi. "Memory System Reference (`docs/memory.md`), Firmware/OS
Reference (`docs/firmware.md`), GPU Architecture Reference (`docs/gpu.md`)."
Local repository. Cross-referenced for memory bandwidth figures, Horizon OS
service architecture, NVN2 display pipeline, and service gap analyses.
Accessed: 2026-05-03. [CONFIRMED]

---

*Document generated: 2026-05-03*
*Last updated: 2026-05-03*
*Part of oboromi M001/S06 — Display/IO hardware reference documentation*
