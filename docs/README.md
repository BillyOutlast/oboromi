# oboromi Documentation Suite — Master Index

> **Target Hardware:** NVIDIA T239 SoC (Nintendo Switch 2)
> **Documentation Scope:** 7 hardware/software domains + cross-domain glossary + unified gap analysis
> **Total Gap Count:** 64 gaps (13 P0, 12 P1, 15 P2, 13 P3, 11 P4)
> **Glossary Terms:** 123 cross-domain entries
> **Last Updated:** 2026-05-03

---

## T239 SoC Architecture Overview

The NVIDIA T239 is a custom system-on-chip fabricated for the Nintendo Switch 2
console. It combines an 8-core ARM Cortex-A78C CPU complex with an Ampere-based
SM86 GPU on a single die, connected to 12 GB of LPDDR5X DRAM via a 128-bit
unified memory interface.

```
+------------------------------------------------------------------+
|                        NVIDIA T239 SoC                           |
|                                                                  |
|  +-------------------+    +----------------------------------+   |
|  |    CPU Complex     |    |         GPU (Ampere SM86)        |   |
|  |  8x ARM Cortex-A78C|    |  12 SMs, 1,536 CUDA cores       |   |
|  |  6 user + 2 system |    |  48 Tensor Cores, 12 RT Cores   |   |
|  |  4MB shared L3     |    |  DLSS, NVN2 graphics API        |   |
|  +-------------------+    +----------------------------------+   |
|                                                                  |
|  +----------------------------------------------------------+   |
|  |              128-bit LPDDR5X (12 GB, 9 GB for games)      |   |
|  +----------------------------------------------------------+   |
|                                                                  |
|  +-------------------+  +------------+  +---------------------+  |
|  |  Storage (UFS 3.1) |  | Security   |  |  Display / IO       |  |
|  |  256 GB + microSD  |  | TrustZone  |  |  1080p LCD / 4K TV  |  |
|  |  Game cards (XCI)  |  | eFuse RoT  |  |  Audio / Wi-Fi / BT |  |
|  +-------------------+  +------------+  +---------------------+  |
|                                                                  |
|  +----------------------------------------------------------+   |
|  |              Horizon OS (L4 microkernel)                  |   |
|  |              HIPC services, NVN2, system modules          |   |
|  +----------------------------------------------------------+   |
+------------------------------------------------------------------+
```

**Figure 1:** T239 SoC block diagram. All subsystems share unified LPDDR5X memory.
Documentation covers each block as an independent domain document.

---

## Documentation Map

### Domain Documents

| Domain | Document | Sections | Lines | Description |
|---|---|---|---|---|
| **GPU** | [docs/gpu.md](gpu.md) | 14 | 1,497 | Ampere SM86 architecture, SASS ISA, RT/Tensor cores, Ada hybrid features, DLSS, NVN2 |
| **CPU** | [docs/cpu.md](cpu.md) | 14 | 1,080 | ARM Cortex-A78C microarchitecture, ARMv8 ISA, cache hierarchy, MMU, GIC, power management |
| **Memory** | [docs/memory.md](memory.md) | 13 | 1,143 | LPDDR5X DRAM specs, memory controller, physical address space, DMA, UMA, coherency |
| **Security** | [docs/security.md](security.md) | 12 | 1,355 | eFuse/OTP, secure boot chain, PKI, TrustZone, ASLR, crypto extensions, TSEC, DRM |
| **Firmware** | [docs/firmware.md](firmware.md) | 12 | 1,246 | Horizon microkernel, HIPC protocol, KIPs, Service Manager, boot sequence, NVN2 API |
| **Display/IO** | [docs/display-io.md](display-io.md) | 17 | 1,810 | LCD panel, display controller, dock, audio, Joy-Con 2, touchscreen, Wi-Fi 6E, BT 5.x, USB-C, NFC |
| **Storage** | [docs/storage.md](storage.md) | 11 | 1,208 | UFS 3.1, partition layout, NCA/NSP/XCI formats, FDE, crypto paths, microSD Express, game cards |

### Cross-Domain References

| Document | Description |
|---|---|
| [docs/glossary.md](glossary.md) | 123-term cross-domain glossary covering GPU, CPU, Memory, Security, Firmware, Display/IO, and Storage terminology |
| [docs/gap-analysis.md](gap-analysis.md) | Unified gap analysis consolidating all 7 domain gaps into a priority-ranked table (P0–P4) with source file mappings and a 7-phase implementation roadmap |

---

## Confidence Summary

Every factual claim in the domain documentation is tagged with one of three
confidence levels:

| Tag | Meaning |
|---|---|
| **CONFIRMED** | Verified from NVIDIA/ARM/Nintendo official documentation, silicon analysis, or oboromi source code |
| **INFERRED** | Derived from closely related public documentation (Orin T234 TRM, Ampere whitepapers, JEDEC specs) |
| **SPECULATIVE** | Based on industry analysis, reverse engineering, or extrapolation from similar parts |

### Per-Domain Confidence Ratings

| Domain | CONFIRMED | INFERRED | SPECULATIVE | Total Tags | Confidence Score |
|---|---|---|---|---|---|
| **GPU** | 185 (74.9%) | 37 (15.0%) | 25 (10.1%) | 247 | ██████████████░░░░░░ High |
| **CPU** | 157 (81.3%) | 32 (16.6%) | 4 (2.1%) | 193 | ████████████████░░░░ Very High |
| **Memory** | 47 (28.1%) | 60 (35.9%) | 60 (35.9%) | 167 | ██████░░░░░░░░░░░░░░ Moderate |
| **Security** | 192 (61.3%) | 97 (31.0%) | 23 (7.3%) | 312 | ████████████░░░░░░░░ High |
| **Firmware** | 132 (71.0%) | 26 (14.0%) | 25 (13.5%) | 183 | ██████████████░░░░░░ High |
| **Display/IO** | 194 (38.9%) | 285 (57.1%) | 80 (16.0%) | 559 | ████████░░░░░░░░░░░░ Moderate |
| **Storage** | 136 (64.5%) | 58 (27.5%) | 24 (11.4%) | 213 | █████████████░░░░░░░ High |
| **Overall** | **1,043 (55.5%)** | **595 (31.7%)** | **241 (12.8%)** | **1,874** | ███████████░░░░░░░░░ High |

**Interpretation:** The CPU and GPU domains have the highest confidence, grounded
in official ARM TRM and NVIDIA Ampere documentation. Memory and Display/IO have
lower confidence due to T239-specific details (memory controller tuning, display
panel specs) that are not publicly documented and must be inferred from the Orin
T234 TRM or community analysis. Security is well-documented thanks to public
NVIDIA Jetson secure boot documentation and Tegra security research.

---

## Gap Analysis Summary

The [docs/gap-analysis.md](gap-analysis.md) identifies **64 gaps** between the
T239 reference documentation and the current oboromi emulator codebase, organized
by priority tier:

| Priority | Count | Definition | Examples |
|---|---|---|---|
| **P0** | 13 | Blocks basic operation | SASS stubs (GPU-01/02), HIPC dispatch (FW-01), NCA parsing (STOR-01), eFuse emulation (SEC-01) |
| **P1** | 12 | Required for OS boot | GICv3 (CPU-03), Service Manager (FW-02), TrustZone (SEC-03), display compositor (DISP-01) |
| **P2** | 15 | Needed for game compat | Texture stubs (GPU-05), DMA engine (MEM-02), Wi-Fi/BT (DISP-05/06), save data (STOR-05) |
| **P3** | 13 | Advanced features | Tensor Core MMA (GPU-07), RT Core TTU (GPU-08), pipeline timing (CPU-08) |
| **P4** | 11 | Accuracy / analysis | Fence/TLD (GPU-09), NVMe emulation (STOR-06), KIP capability analysis (FW-06) |

### Highest-Impact Source Files

| Source File | Gaps | Primary Areas |
|---|---|---|
| `core/src/sys/mod.rs` | 12 | Security (eFuse, TSEC, key derivation), Firmware (handle table), Memory (DMA) |
| `core/src/nn/mod.rs` | 11 | Firmware (service manager), Display/IO (all service stubs) |
| `core/src/gpu/sm86.rs` | 10 | GPU (all SASS instruction stubs, texture, Tensor/RT cores) |
| `core/src/cpu/cpu_manager.rs` | 7 | CPU (memory map), Security (boot chain, ASLR) |

---

## How to Use This Documentation

1. **Start here** (this file) for an overview of the T239 SoC and documentation structure
2. **Read domain docs** independently — each is self-contained with its own table of contents, diagrams, and citations
3. **Look up terms** in [docs/glossary.md](glossary.md) when encountering cross-domain terminology
4. **Plan implementation** using [docs/gap-analysis.md](gap-analysis.md) to prioritize work by P0–P4 tier
5. **Check confidence tags** (CONFIRMED / INFERRED / SPECULATIVE) before relying on any specific claim for emulator code

### Confidence Tag Usage

When writing emulator code based on this documentation:

- **CONFIRMED** claims can be implemented directly
- **INFERRED** claims should be implemented with fallback paths and marked with `// TODO: verify against T239 silicon`
- **SPECULATIVE** claims should be treated as provisional — implement with feature flags or runtime toggles

---

## Sources

Each domain document includes its own numbered citation list. Key cross-domain sources:

| ID | Source | Description | Domains |
|---|---|---|---|
| [1] | Digital Foundry Switch 2 hardware analysis | Die-shot analysis, clock speeds, memory bandwidth | All |
| [2] | NVIDIA Orin T234 TRM (public) | Closest public TRM to T239 — register maps, memory controller, security | All |
| [3] | Nintendo developer documentation | SDK docs, system resource reservations, core allocation | CPU, Firmware, Display/IO |
| [4] | ARM Cortex-A78C TRM | CPU microarchitecture, pipeline, cache, GIC | CPU |
| [5] | NVIDIA Ampere whitepapers | SM86 architecture, RT/Tensor cores, CUDA ISA | GPU |
| [6] | switchbrew wiki | Horizon OS internals, IPC protocol, kernel objects | Firmware |
| [7] | Atmosphère source code | Reverse-engineered kernel, service manager, boot chain | Firmware, Security |
| [8] | JEDEC specifications | LPDDR5X (JESD209-5), UFS 3.1 | Memory, Storage |

---

*Generated as part of oboromi M001/S08 — T239 SoC Documentation Suite.*
