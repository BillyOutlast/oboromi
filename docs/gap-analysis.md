# oboromi Unified Gap Analysis

This document consolidates the gap analysis sections from all 7 domain
documentation files into a single priority-ranked view. Each domain's
gaps are mapped to a cross-domain priority tier (P0–P4) and linked to
oboromi source files, enabling milestone-level planning.

**Source documents:**

- `docs/gpu.md` §13 — GPU Architecture (Ampere SM86 / SPIR-V)
- `docs/cpu.md` §13 — CPU Architecture (Cortex-A78C)
- `docs/memory.md` §12 — Memory System (LPDDR5X / controller)
- `docs/security.md` §11 — Security (eFuse / TrustZone / boot chain)
- `docs/firmware.md` §10 — Firmware & OS (HIPC / services / kernel)
- `docs/display-io.md` §15 — Display & I/O (display / audio / input / networking)
- `docs/storage.md` §10 — Storage (UFS / NCA / encryption)

---

## 1. Cross-Domain Priority Summary

The following table ranks every identified gap across all 7 domains by
cross-domain priority. Priority tiers are defined as:

| Tier | Definition | Blocking? |
|---|---|---|
| **P0** | Blocks basic functional operation — cannot boot or render without it | Yes |
| **P1** | Required for OS boot and driver communication — core services fail without it | Yes |
| **P2** | Needed for game compatibility — many titles will fail or degrade | Partial |
| **P3** | Needed for advanced features — niche or performance-oriented | No |
| **P4** | Accuracy / analysis features — not needed for functional emulation | No |

### 1.1 Master Priority Table

| Priority | Domain | Gap ID | Gap Description | Effort | Source Files |
|---|---|---|---|---|---|
| **P0** | GPU | GPU-01 | Implement core arithmetic SASS stubs (FADD, FFMA, IADD3, IMAD, LOP3, MOV, SEL) | Medium | `core/src/gpu/sm86.rs` |
| **P0** | GPU | GPU-02 | Implement memory SASS stubs (LDG, STG, LDS, STS, LD, ST) | Medium | `core/src/gpu/sm86.rs` |
| **P0** | CPU | CPU-01 | Memory map (address space layout for MMIO / GPU registers) | Medium | `core/src/cpu/cpu_manager.rs` |
| **P0** | Display/IO | DISP-01 | Display compositor (`vi`, `disp` services) | High | `core/src/nn/mod.rs`, `core/src/sys/mod.rs` |
| **P0** | Display/IO | DISP-02 | HID input (`hid`, `hidbus` services) | High | `core/src/nn/mod.rs`, `core/src/sys/mod.rs` |
| **P0** | Firmware | FW-01 | HIPC message dispatch loop | Medium | `core/src/nn/hipc.rs` |
| **P0** | Storage | STOR-01 | NCA container parsing | Large | `core/src/fs/mod.rs` (new) |
| **P0** | Storage | STOR-02 | BIS key derivation and decryption | Medium | `core/src/fs/mod.rs` (new), `core/src/sys/mod.rs` |
| **P0** | Storage | STOR-03 | XCI/NSP format support | Medium | `core/src/fs/mod.rs` (new) |
| **P0** | Security | SEC-01 | eFuse/OTP emulation | Medium | `core/src/sys/mod.rs` (new) |
| **P0** | Security | SEC-02 | BootROM / Secure Boot chain | High | `core/src/cpu/cpu_manager.rs`, `core/src/lib.rs` |
| **P0** | Security | SEC-09 | Key derivation (SBK → SSK → device keys) | Medium | `core/src/sys/mod.rs` (new) |
| **P1** | GPU | GPU-03 | Implement control flow stubs (BRA, BRX, CALL, RET, EXIT, WARPSYNC) | Medium | `core/src/gpu/sm86.rs` |
| **P1** | GPU | GPU-04 | Implement predicated execution for remaining instructions | Small | `core/src/gpu/sm86.rs` |
| **P1** | CPU | CPU-02 | Cache simulation (L1/L2/L3 timing) | High | `core/src/cpu/` (new) |
| **P1** | CPU | CPU-03 | Interrupt controller (GICv3) | High | `core/src/cpu/` (new) |
| **P1** | CPU | CPU-04 | Exception levels (EL0–EL3) | High | `core/src/cpu/unicorn_interface.rs` |
| **P1** | CPU | CPU-05 | MMU with real translation tables | High | `core/src/cpu/` (new) |
| **P1** | Memory | MEM-01 | MMIO address regions (memory-mapped device framework) | Medium | `core/src/cpu/cpu_manager.rs` |
| **P1** | Security | SEC-03 | TrustZone (EL3/S-EL1, world switching, SMC handler) | High | `core/src/cpu/unicorn_interface.rs` |
| **P1** | Firmware | FW-02 | Service Manager (sm) — service discovery and session routing | Medium | `core/src/nn/mod.rs` |
| **P1** | Firmware | FW-03 | Handle table — kernel object reference counting | Medium | `core/src/sys/mod.rs` (new) |
| **P1** | Firmware | FW-04 | KIP loader (INI1 parsing, segment loading, capability extraction) | Large | `core/src/sys/mod.rs` (new) |
| **P1** | Display/IO | DISP-03 | Audio output (`audout`, `aud` services) | High | `core/src/nn/mod.rs`, `core/src/sys/mod.rs` |
| **P1** | Display/IO | DISP-04 | Touchscreen (`ts`, `tspm` services) | Medium | `core/src/nn/mod.rs` |
| **P2** | GPU | GPU-05 | Implement texture/surface stubs (TEX, TLD, SULD, SUST) | Large | `core/src/gpu/sm86.rs` |
| **P2** | GPU | GPU-06 | Implement warp-level ops (VOTE, SHFL, REDUX, MATCH) | Medium | `core/src/gpu/sm86.rs` |
| **P2** | CPU | CPU-06 | Generic Timer | Medium | `core/src/cpu/` (new) |
| **P2** | CPU | CPU-07 | Crypto Extensions (AES/SHA/CRC32) | Low | `core/src/cpu/unicorn_interface.rs` |
| **P2** | Memory | MEM-02 | DMA engine emulation (GPC-DMA) | Medium | `core/src/sys/mod.rs` (new) |
| **P2** | Security | SEC-04 | TSEC/Falcon µP emulation | Large | `core/src/sys/mod.rs` (new) |
| **P2** | Security | SEC-05 | Crypto Extension instruction emulation | Medium | `core/src/cpu/unicorn_interface.rs` |
| **P2** | Security | SEC-07 | ASLR implementation | Medium | `core/src/cpu/cpu_manager.rs` |
| **P2** | Security | SEC-11 | Anti-rollback (monotonic eFuse counter) | Medium | `core/src/sys/mod.rs` |
| **P2** | Security | SEC-14 | Game integrity verification | Medium | `core/src/fs/mod.rs` |
| **P2** | Firmware | FW-05 | NVN2/nvdrv command submission bridge | Large | `core/src/gpu/` (new) |
| **P2** | Display/IO | DISP-05 | Wi-Fi (`wlan`, `nifm` services) | High | `core/src/nn/mod.rs` |
| **P2** | Display/IO | DISP-06 | Bluetooth (`bt`, `btdrv`, `btm` services) | High | `core/src/nn/mod.rs` |
| **P2** | Storage | STOR-04 | FDE simulation (software LZ4 fallback) | Small | `core/src/fs/mod.rs` |
| **P2** | Storage | STOR-05 | Save data filesystem API | Medium | `core/src/fs/mod.rs`, `core/src/nn/mod.rs` |
| **P3** | GPU | GPU-07 | Implement Tensor Core MMA stubs (HMMA, IMMA, DMMA) | Large | `core/src/gpu/sm86.rs` |
| **P3** | GPU | GPU-08 | Implement RT Core TTU stubs | Large | `core/src/gpu/sm86.rs` |
| **P3** | CPU | CPU-08 | Pipeline timing model | Very High | `core/src/cpu/` (new) |
| **P3** | CPU | CPU-09 | Power management (DVFS) | Low | `core/src/cpu/` (new) |
| **P3** | CPU | CPU-10 | Debug infrastructure (breakpoints/watchpoints) | Medium | `core/src/cpu/` (new) |
| **P3** | Memory | MEM-03 | Cache hierarchy simulation | Large | `core/src/cpu/` (new) |
| **P3** | Memory | MEM-04 | Coherency model (MESI + IOMMU) | Large | `core/src/cpu/` (new) |
| **P3** | Security | SEC-13 | Secure Video Path (VPR memory carve-out) | Medium | `core/src/cpu/cpu_manager.rs`, `core/src/lib.rs` |
| **P3** | Display/IO | DISP-07 | USB host stack (`usb` service) | Medium | `core/src/nn/mod.rs` |
| **P3** | Display/IO | DISP-08 | NFC (`nfc`, `nfp` services) | Medium | `core/src/nn/mod.rs` |
| **P3** | Display/IO | DISP-09 | Ethernet (`eth`, `ethc` services) | Medium | `core/src/nn/mod.rs` |
| **P3** | Storage | STOR-06 | microSD Express emulation | Small | `core/src/fs/mod.rs` |
| **P3** | Storage | STOR-07 | Game card authentication | Medium | `core/src/fs/mod.rs` |
| **P4** | GPU | GPU-09 | Shared memory bank conflict modeling | Medium | `core/src/gpu/` (new) |
| **P4** | GPU | GPU-10 | Occupancy / register pressure tracking | Medium | `core/src/gpu/` (new) |
| **P4** | Memory | MEM-05 | Memory controller timing (MC refresh, bank interleaving) | Large | `core/src/sys/mod.rs` (new) |
| **P4** | Memory | MEM-06 | ECC simulation | Large | `core/src/sys/mod.rs` (new) |
| **P4** | Security | SEC-06 | Memory encryption (on-the-fly DRAM encrypt/decrypt) | Large | `core/src/cpu/cpu_manager.rs` |
| **P4** | Security | SEC-08 | PAC/MTE support | Medium | `core/src/cpu/unicorn_interface.rs` |
| **P4** | Security | SEC-10 | DRM / content protection | Large | `core/src/sys/mod.rs` (new) |
| **P4** | Security | SEC-12 | HDCP key management (KFUSE) | Medium | `core/src/sys/mod.rs` (new) |
| **P4** | Display/IO | DISP-10 | Camera (`vic`, `capmtp` services) | Low | `core/src/nn/mod.rs` |
| **P4** | Display/IO | DISP-11 | Codec control (`codecctl` service) | Low | `core/src/nn/mod.rs` |
| **P4** | Storage | STOR-08 | Cloud save sync | — | Out of scope |

---

## 2. Domain: GPU (Ampere SM86 / SPIR-V)

**Source:** `docs/gpu.md` §13

### 2.1 Implementation Status

| Category | Total | Implemented | Stubbed | Coverage |
|---|---|---|---|---|
| SASS instruction handlers | 206 | 11 | 195 | 5.3% |
| SPIR-V emit functions | ~150 | ~150 | 0 | ~100% |
| Decoder dispatch entries | 1,271 | 1,271 | 0 | 100% |
| Register types modeled | 7 | 2 | 5 | 28.6% |
| Memory hierarchy levels | 6 | 0 | 6 | 0% |
| RT Core instructions | 7 | 0 | 7 | 0% |
| Tensor Core instructions | 6 | 0 | 6 | 0% |
| Texture/surface instructions | 13 | 0 | 13 | 0% |
| Barrier instructions | 6 | 0 | 6 | 0% |
| Control flow instructions | 14 | 0 | 14 | 0% |

**Key finding:** The SASS decoder and SPIR-V emitter are complete and
functional. The critical gap is execution/transformation logic — 195 of
206 instruction handlers are `todo!()` stubs. The decoder correctly
dispatches all 1,271 instruction variants, but instructions are not
executed.

### 2.2 Priority Gaps

| Priority | Gap | Effort | Impact |
|---|---|---|---|
| P0 | Core arithmetic stubs (FADD, FFMA, IADD3, IMAD, LOP3, MOV, SEL) | Medium | Enables basic SASS execution |
| P0 | Memory stubs (LDG, STG, LDS, STS, LD, ST) | Medium | Enables memory access simulation |
| P1 | Control flow (BRA, BRX, CALL, RET, EXIT, WARPSYNC) | Medium | Enables multi-block programs |
| P1 | Predicated execution for remaining instructions | Small | Enables real shader translation |
| P2 | Texture/surface stubs (TEX, TLD, SULD, SUST) | Large | Enables graphics shader analysis |
| P2 | Warp-level ops (VOTE, SHFL, REDUX, MATCH) | Medium | Enables compute shader support |
| P3 | Tensor Core MMA stubs (HMMA, IMMA, DMMA) | Large | Enables DLSS/workflow analysis |
| P3 | RT Core TTU stubs | Large | Enables ray tracing analysis |
| P4 | Shared memory bank conflict modeling | Medium | Performance analysis |
| P4 | Occupancy / register pressure tracking | Medium | Performance analysis |

### 2.3 Source Files

| File | Content | Lines |
|---|---|---|
| `core/src/gpu/sm86.rs` | SASS decoder + instruction implementations | 4,208 |
| `core/src/gpu/spirv.rs` | SPIR-V code emitter | 1,080 |
| `core/src/gpu/sm86_decoder_generated.rs` | Auto-generated decoder dispatch | 2,552 |
| `core/src/gpu/mod.rs` | Module root and GPU state | 63 |

---

## 3. Domain: CPU (Cortex-A78C)

**Source:** `docs/cpu.md` §13

### 3.1 Implementation Status

The oboromi CPU manager provides a functional 8-core ARM64 execution
environment via the Unicorn Engine, but lacks all hardware-specific
features. Core count and endianness match the T239; everything else
is either simplified or unimplemented.

| Component | Status | Gap |
|---|---|---|
| Core count | ✅ 8 cores | None |
| Architecture | ✅ ARM64 | None |
| Memory model | ⚠️ 12 GB flat | Real T239 has complex address map |
| Stack allocation | ⚠️ 1 MB per core | Simplified vs real layout |
| L1/L2/L3 Cache | ❌ Not implemented | No cache simulation |
| MMU/TLB | ❌ Not implemented | Unicorn handles translation internally |
| GIC | ❌ Not implemented | No interrupt controller |
| Generic Timer | ❌ Not implemented | No timer simulation |
| Crypto Extensions | ⚠️ Depends on Unicorn | May or may not be supported |
| Exception levels | ❌ Not implemented | No EL simulation |
| Cache coherency | ❌ Not implemented | No MESI protocol |
| DVFS / Clock gating | ❌ Not implemented | No power management |
| Pipeline simulation | ❌ Not implemented | No cycle-accurate model |
| Performance counters | ❌ Not implemented | No PMU |

### 3.2 Priority Gaps

| Priority | Gap | Effort | Impact |
|---|---|---|---|
| P0 | Memory map (address space layout) | Medium | Required for MMIO, GPU registers |
| P0 | Cache simulation (at least timing) | High | Required for accurate performance |
| P1 | Interrupt controller (GICv3) | High | Required for OS boot, driver communication |
| P1 | Exception levels (EL0–EL3) | High | Required for OS/hypervisor separation |
| P1 | MMU with real translation tables | High | Required for virtual memory |
| P2 | Generic Timer | Medium | Required for OS scheduling |
| P2 | Crypto Extensions | Low | Required for secure content |
| P3 | Pipeline timing model | Very High | Required for cycle-accurate emulation |
| P3 | Power management | Low | Required for power state transitions |
| P3 | Debug infrastructure | Medium | Required for debugging tools |

### 3.3 Source Files

| File | Content | Lines |
|---|---|---|
| `core/src/cpu/cpu_manager.rs` | 8-core CPU manager, 12GB memory, round-robin | ~60 |
| `core/src/cpu/unicorn_interface.rs` | Unicorn Engine wrapper, register/memory access | ~250 |

---

## 4. Domain: Memory (LPDDR5X / Controller)

**Source:** `docs/memory.md` §12

### 4.1 Implementation Status

oboromi implements a simplified flat memory model with 12 GB shared
memory at address 0x0 (real T239 base is 0x8000_0000). The current
model provides functional correctness for basic emulation but lacks
all hardware-specific memory features.

| Feature | T239 Actual | oboromi | Gap |
|---|---|---|---|
| Memory size | 12 GB | 12 GB | ✅ Match |
| Base address | 0x8000_0000 | 0x0 | ⚠️ Simplified |
| Channels | 2 × 64-bit | Flat | ❌ Not modeled |
| Bank interleaving | 16 banks | None | ❌ Not modeled |
| QoS arbitration | Priority-based | Sequential | ❌ Not modeled |
| DMA engines | GPC-DMA, GPU CE, Video DMA | None | ❌ Not modeled |
| Cache hierarchy | L1/L2/L3 | None | ❌ Not modeled |
| Coherency | MESI + IOMMU | Shared pointer | ❌ Not modeled |
| MMIO regions | Multiple apertures | None | ❌ Not modeled |
| System reservation | 3 GB OS / 9 GB game | None | ⚠️ Simplified |
| Stack layout | 1 MB per core | 1 MB per core | ✅ Match |
| Power/DVFS | 2 speed grades | None | ❌ Not modeled |
| ECC | On-die + link | None | ❌ Not modeled |

### 4.2 Priority Gaps

| Priority | Gap | Effort | Rationale |
|---|---|---|---|
| P1 | MMIO address regions | Medium | Required for GPU/peripheral register access |
| P2 | DMA engine emulation (GPC-DMA) | Medium | Games use DMA for asset loading |
| P3 | Cache hierarchy simulation | Large | Affects timing, not correctness |
| P3 | Coherency model | Large | Current shared-pointer may suffice |
| P4 | Memory controller timing | Large | Only needed for timing-accurate emulation |
| P4 | ECC simulation | Large | Only needed for fault injection testing |

### 4.3 Source Files

| File | Content | Notes |
|---|---|---|
| `core/src/cpu/cpu_manager.rs` | Memory allocation (12 GB, 0x0 base) | Memory model lives here |

---

## 5. Domain: Security (eFuse / TrustZone / Boot Chain)

**Source:** `docs/security.md` §11

### 5.1 Implementation Status

The oboromi codebase has **zero security infrastructure**. All 14
security domains are unimplemented. The highest-priority gaps
(SEC-01, SEC-02, SEC-03, SEC-09) are prerequisites for booting any
signed Switch 2 software.

| Gap ID | Domain | Status | Priority |
|---|---|---|---|
| SEC-01 | eFuse/OTP emulation | ❌ Not implemented | High |
| SEC-02 | BootROM / Secure Boot | ❌ Not implemented | High |
| SEC-03 | TrustZone (EL3/S-EL1) | ❌ Not implemented | High |
| SEC-04 | TSEC/Falcon µP | ❌ Not implemented | Medium |
| SEC-05 | Crypto Extensions | ❌ Not implemented | Medium |
| SEC-06 | Memory Encryption | ❌ Not implemented | Low |
| SEC-07 | ASLR | ❌ Not implemented | Medium |
| SEC-08 | PAC/MTE | ❌ Not implemented | Low |
| SEC-09 | Key Derivation | ❌ Not implemented | High |
| SEC-10 | DRM/Content Protection | ❌ Not implemented | Low |
| SEC-11 | Anti-Rollback | ❌ Not implemented | Medium |
| SEC-12 | HDCP Key Management | ❌ Not implemented | Low |
| SEC-13 | Secure Video Path | ❌ Not implemented | Low |
| SEC-14 | Game Integrity | ❌ Not implemented | Medium |

### 5.2 Implementation Recommendations

| Phase | Focus | Dependencies |
|---|---|---|
| Phase 1 | Crypto foundation (AES-256, SHA-256) | None |
| Phase 2 | Key derivation chain (SBK → SSK → device keys) | Phase 1 |
| Phase 3 | Boot chain emulation (BootROM → BCT → IBB → OBB) | Phase 2 |
| Phase 4 | TrustZone (EL3/EL2, SMC handling) | Phase 3 |
| Phase 5 | TSEC/Falcon µP (deferred) | Phase 4 |

### 5.3 Source File Mapping

| Source File | Security Relevance | Gap IDs |
|---|---|---|
| `core/src/cpu/cpu_manager.rs` | Memory encryption, ASLR, VPR carve-out | SEC-02, SEC-06, SEC-07, SEC-13 |
| `core/src/cpu/unicorn_interface.rs` | TrustZone, crypto extensions, PAC/MTE | SEC-03, SEC-05, SEC-08 |
| `core/src/fs/mod.rs` | Game integrity verification | SEC-14 |
| `core/src/sys/mod.rs` | Primary target for new security modules | SEC-01, SEC-04, SEC-09, SEC-10, SEC-11, SEC-12 |
| `core/src/lib.rs` | Secure boot initialization | SEC-02, SEC-13 |

---

## 6. Domain: Firmware & OS (HIPC / Services / Kernel)

**Source:** `docs/firmware.md` §10

### 6.1 Implementation Status

oboromi has basic HIPC header parsing and 160 service name stubs, but
no functional service logic. Every service initializes but performs no
work. Critical IPC and kernel infrastructure is entirely missing.

| Component | oboromi Status | Gap |
|---|---|---|
| HIPC protocol | Header parsing only, no dispatch | ❌ No command routing |
| Service registry | 160 named stubs, all `todo!()` | ❌ No functional logic |
| Service Manager (sm) | Not implemented | ❌ No discovery / session routing |
| Handle table | Not implemented | ❌ No object lifecycle management |
| KIP/INI1 parsing | Not implemented | ❌ No system process loading |
| Boot sequence | Not implemented | ❌ No Package1/Package2 loading |
| Kernel scheduler | Not implemented | ❌ No priority scheduling |
| NVN2 command submission | Not implemented | ❌ No GPU command buffer path |
| Sleep/resume | Not implemented | ❌ No warmboot handling |
| Kernel capabilities | Not implemented | ❌ No syscall mask enforcement |

### 6.2 Priority Gaps

| Priority | Gap | Effort | Rationale |
|---|---|---|---|
| P0 | HIPC dispatch loop | Medium | Enables service stubs to receive IPC calls |
| P1 | Service Manager (sm) | Medium | Enables service discovery and session routing |
| P1 | Handle table | Medium | Enables proper kernel object lifecycle |
| P1 | KIP loader (INI1 parsing) | Large | Required to load system process images |
| P2 | NVN2/nvdrv bridge | Large | Enables GPU command submission |

### 6.3 Source Files

| File | Content | Lines |
|---|---|---|
| `core/src/nn/hipc.rs` | HIPC header parsing | 44 |
| `core/src/nn/mod.rs` | 160 service stubs, service trait | 225 |
| `core/src/sys/mod.rs` | System state management | 179 |
| `core/src/fs/mod.rs` | File system abstraction (mmap) | 25 |

---

## 7. Domain: Display & I/O

**Source:** `docs/display-io.md` §15

### 7.1 Implementation Status

All 13 display/IO feature areas have service stubs defined via
`define_service!` macros but lack any functional implementation.
There are 33 stubs total with zero functional logic.

| Feature | Services | Stubs | Status |
|---|---|---|---|
| Display compositor | `vi`, `vi2`, `disp`, `dispdrv`, `ommdisp` | 5 | Stub |
| Audio renderer | `aud`, `audout`, `audin`, `audren`, `audrec`, `audsmx`, `audctl`, `hwopus` | 8 | Stub |
| HID input | `hid`, `hidbus`, `ahid` | 3 | Stub |
| Touchscreen | `ts`, `tspm` | 2 | Stub |
| Wi-Fi | `wlan`, `nifm` | 2 | Stub |
| Bluetooth | `bt`, `btdrv`, `btm`, `btp` | 4 | Stub |
| USB | `usb` | 1 | Stub |
| NFC | `nfc`, `nfp` | 2 | Stub |
| Ethernet | `eth`, `ethc` | 2 | Stub |
| Camera | `vic`, `capmtp` | 2 | Stub |
| I2C bus | `i2c` | 1 | Stub |
| Codec control | `codecctl` | 1 | Stub |
| Network config | `sfdnsres`, `ssl` | 2 | Stub |

### 7.2 Priority Gaps

| Priority | Feature Area | Effort | Rationale |
|---|---|---|---|
| P0 | Display compositor (`vi`, `disp`) | High | Required for any screen output |
| P0 | HID input (`hid`, `hidbus`) | High | Required for any controller input |
| P1 | Audio output (`audout`, `aud`) | High | Required for sound |
| P1 | Touchscreen (`ts`) | Medium | Required for handheld touch input |
| P2 | Wi-Fi (`wlan`, `nifm`) | High | Required for online features |
| P2 | Bluetooth (`bt`, `btdrv`, `btm`) | High | Required for wireless controllers |
| P3 | USB (`usb`) | Medium | Required for dock peripherals |
| P3 | NFC (`nfc`, `nfp`) | Medium | Required for amiibo |
| P3 | Ethernet (`eth`) | Medium | Required for wired LAN |
| P4 | Camera (`vic`) | Low | Required for GameChat video |
| P4 | Codec (`codecctl`) | Low | Required for video playback |

---

## 8. Domain: Storage (UFS / NCA / Encryption)

**Source:** `docs/storage.md` §10

### 8.1 Implementation Status

oboromi has a minimal filesystem abstraction (`memmap2`-based memory-mapped
file I/O). There is no emulated storage controller, no NCA/XCI parsing,
no encryption, and no FDE simulation.

| Feature | T239 Actual | oboromi | Gap |
|---|---|---|---|
| UFS 3.1 controller | Full UFS HC with command queue | Host filesystem passthrough | ❌ Not modeled |
| NCA container format | Full NCA parsing, key area decryption | None | ❌ Not modeled |
| NSP bundle format | Multi-NCA container | None | ❌ Not modeled |
| XCI game card format | HFS0 + NCA container | None | ❌ Not modeled |
| BIS encryption | AES-XTS, per-console keys | None | ❌ Not modeled |
| FDE | Hardware LZ4, DMA-based | Software only (lz4 crate) | ⚠️ Software fallback |
| microSD Express | PCIe Gen3 + NVMe | None | ❌ Not modeled |
| Game card interface | Custom serial, AES-128-CTR | None | ❌ Not modeled |
| Save data | Per-user, per-title, BIS-encrypted | Direct file writes | ❌ Not modeled |
| Memory-mapped I/O | Via UFS driver + page cache | memmap2 crate | ✅ Basic equivalent |

### 8.2 Priority Gaps

| Priority | Gap | Effort | Dependencies |
|---|---|---|---|
| P0 | NCA container parsing | Large | None |
| P0 | BIS key derivation and decryption | Medium | Key hierarchy (security module) |
| P0 | XCI/NSP format support | Medium | NCA parsing |
| P2 | FDE simulation (software LZ4 fallback) | Small | Memory system DMA path |
| P2 | Save data filesystem API | Medium | HIPC service framework |
| P3 | microSD Express emulation | Small | fs-sysmodule |
| P3 | Game card authentication | Medium | Security module |

### 8.3 Source Files

| File | Content | Lines |
|---|---|---|
| `core/src/fs/mod.rs` | File system abstraction (mmap) | 25 |

---

## 9. Cross-Domain Dependency Map

Many gaps have cross-domain dependencies. The following map shows which
gates must be completed before others can begin.

```
Crypto Foundation (SEC-05)
  ├── Key Derivation (SEC-09)
  │     ├── eFuse/OTP (SEC-01)
  │     ├── BIS Decryption (STOR-02)
  │     └── Secure Boot (SEC-02)
  │           └── Boot Sequence (FW-boot)
  │                 ├── Service Manager (FW-02)
  │                 │     ├── HIPC Dispatch (FW-01)
  │                 │     │     ├── Display (DISP-01)
  │                 │     │     ├── Input (DISP-02)
  │                 │     │     ├── Audio (DISP-03)
  │                 │     │     └── All other services
  │                 │     └── Handle Table (FW-03)
  │                 ├── KIP Loader (FW-04)
  │                 │     └── TrustZone (SEC-03)
  │                 └── MMIO Map (CPU-01, MEM-01)
  │                       ├── GPU Execution (GPU-01, GPU-02)
  │                       └── DMA (MEM-02)
  └── NCA Parsing (STOR-01)
        └── XCI/NSP (STOR-03)
```

**Critical path:** Crypto Foundation → Key Derivation → Secure Boot →
Boot Sequence → Service Manager → HIPC Dispatch → [Display, Input, Audio].

---

## 10. Milestone-Level Implementation Roadmap

### Phase 1: Crypto & Boot Foundation (P0 security)

| Task | Gap IDs | Effort | Enables |
|---|---|---|---|
| Crypto library (AES-256, SHA-256) | SEC-05 | Medium | All security features |
| Key derivation module | SEC-09 | Medium | BIS, secure boot, DRM |
| eFuse/OTP emulation | SEC-01 | Medium | Anti-rollback, fuse reads |
| Secure boot chain | SEC-02 | High | OS boot, signed code loading |
| Boot sequence (Package1/2, kernel init) | FW-boot | High | Everything |

### Phase 2: Core Services & IPC (P0 firmware + display)

| Task | Gap IDs | Effort | Enables |
|---|---|---|---|
| HIPC dispatch loop | FW-01 | Medium | Service communication |
| Service Manager (sm) | FW-02 | Medium | Service discovery |
| Handle table | FW-03 | Medium | Object lifecycle |
| Display compositor stub | DISP-01 | High | Screen output |
| HID input stub | DISP-02 | High | Controller input |
| Memory map (address space) | CPU-01, MEM-01 | Medium | MMIO, GPU registers |

### Phase 3: Storage & Content Loading (P0 storage)

| Task | Gap IDs | Effort | Enables |
|---|---|---|---|
| NCA parser | STOR-01 | Large | Game content loading |
| BIS decryption | STOR-02 | Medium | Encrypted content |
| XCI/NSP support | STOR-03 | Medium | Game distribution formats |
| KIP loader | FW-04 | Large | System process images |

### Phase 4: GPU Execution (P0–P1 GPU)

| Task | Gap IDs | Effort | Enables |
|---|---|---|---|
| Core arithmetic stubs | GPU-01 | Medium | Basic shader execution |
| Memory stubs | GPU-02 | Medium | Memory access simulation |
| Control flow stubs | GPU-03 | Medium | Multi-block programs |
| Predicated execution | GPU-04 | Small | Real shader translation |

### Phase 5: Core OS Features (P1–P2)

| Task | Gap IDs | Effort | Enables |
|---|---|---|---|
| TrustZone (EL3/S-EL1) | SEC-03 | High | Secure content |
| GICv3 interrupt controller | CPU-03 | High | OS boot |
| Exception levels | CPU-04 | High | OS/hypervisor separation |
| MMU with real tables | CPU-05 | High | Virtual memory |
| Audio output | DISP-03 | High | Sound |
| Touchscreen | DISP-04 | Medium | Touch input |
| Save data filesystem | STOR-05 | Medium | Game saves |

### Phase 6: Advanced Features (P2–P3)

| Task | Gap IDs | Effort | Enables |
|---|---|---|---|
| Texture/surface stubs | GPU-05 | Large | Graphics shaders |
| Warp-level ops | GPU-06 | Medium | Compute shaders |
| Wi-Fi / Bluetooth | DISP-05, DISP-06 | High | Online features |
| NVN2/nvdrv bridge | FW-05 | Large | GPU command submission |
| TSEC/Falcon µP | SEC-04 | Large | Advanced security |
| Generic Timer | CPU-06 | Medium | OS scheduling |
| DMA engines | MEM-02 | Medium | Asset streaming |

### Phase 7: Accuracy & Performance (P3–P4)

| Task | Gap IDs | Effort | Enables |
|---|---|---|---|
| Cache simulation | CPU-02, MEM-03 | High/Large | Timing accuracy |
| Pipeline timing model | CPU-08 | Very High | Cycle-accurate emulation |
| Tensor Core stubs | GPU-07 | Large | DLSS analysis |
| RT Core stubs | GPU-08 | Large | Ray tracing analysis |
| Memory controller timing | MEM-05 | Large | Bandwidth accuracy |
| Power management | CPU-09 | Low | Power state transitions |
| USB / NFC / Ethernet | DISP-07–09 | Medium | Peripheral support |

---

## 11. Gap Count Summary by Domain

| Domain | Total Gaps | P0 | P1 | P2 | P3 | P4 |
|---|---|---|---|---|---|---|
| GPU | 10 | 2 | 2 | 2 | 2 | 2 |
| CPU | 10 | 2 | 3 | 2 | 3 | 0 |
| Memory | 6 | 0 | 1 | 1 | 2 | 2 |
| Security | 14 | 3 | 1 | 5 | 1 | 4 |
| Firmware | 5 | 1 | 3 | 1 | 0 | 0 |
| Display/IO | 11 | 2 | 2 | 2 | 3 | 2 |
| Storage | 8 | 3 | 0 | 2 | 2 | 1 |
| **Total** | **64** | **13** | **12** | **15** | **13** | **11** |

---

## 12. Source File Impact Heatmap

The following shows which oboromi source files are most affected by
the consolidated gap analysis, based on how many gap IDs reference each file.

| Source File | Referenced By (Gap Count) | Primary Gap IDs |
|---|---|---|
| `core/src/sys/mod.rs` | 12 gaps | SEC-01, SEC-04, SEC-09, SEC-10, SEC-11, SEC-12, FW-03, MEM-02, MEM-05, MEM-06 |
| `core/src/cpu/cpu_manager.rs` | 7 gaps | CPU-01, SEC-02, SEC-06, SEC-07, SEC-13, MEM-01, GPU-* (indirect) |
| `core/src/gpu/sm86.rs` | 10 gaps | GPU-01 through GPU-10 |
| `core/src/nn/mod.rs` | 11 gaps | FW-02, DISP-01 through DISP-11 |
| `core/src/cpu/unicorn_interface.rs` | 5 gaps | CPU-04, SEC-03, SEC-05, SEC-08, CPU-07 |
| `core/src/fs/mod.rs` | 6 gaps | STOR-01 through STOR-08, SEC-14 |
| `core/src/nn/hipc.rs` | 1 gap | FW-01 |
| `core/src/lib.rs` | 2 gaps | SEC-02, SEC-13 |

---

*Document generated as part of oboromi M001/S08. This unified gap analysis
consolidates findings from 7 domain documentation files, covering 64
identified gaps across 12 priority tiers.*
