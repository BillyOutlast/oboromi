# Cross-Domain Glossary: NVIDIA T239 (Switch 2)

> **Scope:** Technical terminology used across GPU, CPU, Memory, Security, Firmware,
> Display/IO, and Storage domain documentation for the NVIDIA T239 SoC.
> **Entry Count:** 120 terms
> **Last Updated:** 2026-05-03

---

### AArch64
64-bit ARM execution state providing 31 general-purpose 64-bit registers (X0–X30), SIMD/FP registers, and system register access. The primary execution state for all Switch 2 software.
**Domains:** CPU, Security, Firmware

### AES (Advanced Encryption Standard)
Symmetric block cipher used throughout the T239 for storage encryption (AES-XTS), content protection (AES-CTR), and secure boot key operations (AES-256). Hardware-accelerated via ARM Cryptographic Extensions.
**Domains:** Security, Storage, CPU

### AES-CTR
AES in Counter Mode — a stream cipher mode used for NCA content section encryption. Each section uses a per-section key with a section-specific tweak.
**Domains:** Security, Storage

### AES-XTS
AES in XEX-based Tweaked-codebook mode with ciphertext Stealing (IEEE 1619). Industry-standard full-disk encryption mode used for BIS (Built-in Storage) encryption on UFS. Supports random-access reads at sector level.
**Domains:** Security, Storage, Memory

### Ampere
NVIDIA GPU microarchitecture (2020) forming the basis of the T239 GPU. SM86 compute capability with 2nd-gen RT Cores, 3rd-gen Tensor Cores, and select Ada Lovelace hybrid features.
**Domains:** GPU

### Anti-Rollback
Mechanism using eFuse-based monotonic counters to prevent firmware downgrade attacks. Each boot stage has a minimum version counter; firmware must declare a version ≥ the fused counter.
**Domains:** Security, Firmware

### ASIMD (Advanced SIMD)
ARM's 128-bit SIMD extension (also called NEON). Each A78C core has two 128-bit ASIMD/FP pipelines supporting vector arithmetic on 8/16/32/64-bit integer and half/single/double-precision FP data.
**Domains:** CPU

### ASLR (Address Space Layout Randomization)
Memory protection technique that randomizes base addresses of code, stack, heap, and libraries at each execution, making exploitation of memory corruption bugs significantly harder.
**Domains:** Security, CPU

### Bank Interleaving
Memory controller technique that accesses different DRAM banks in parallel to hide row activation latency (tRCD). Sequential addresses alternate between banks and channels for maximum bandwidth.
**Domains:** Memory

### Barrier Register
SM86 synchronization primitive (B0–B63) for inter-warp synchronization. Supports SYNC, ARRIVE, RED, SCAN, and SYNCALL operations.
**Domains:** GPU

### BCT (Boot Configuration Table)
Signed configuration blob read by the BootROM containing SDRAM initialization parameters, IBB load addresses, signature verification metadata, and anti-rollback version counters.
**Domains:** Security, Firmware

### BIS (Built-in Storage)
Nintendo's full-disk encryption scheme for internal UFS storage. Uses AES-XTS with per-console keys derived from the TSEC/BPMP key hierarchy. Every byte on flash is encrypted.
**Domains:** Storage, Security

### BLE (Bluetooth Low Energy)
Wireless protocol used for Joy-Con 2 controller communication with the Switch 2 console.
**Domains:** Display/IO

### BootROM
Immutable on-die ROM code that serves as the hardware root of trust for the secure boot chain. Laser-etched into silicon during manufacturing; cannot be modified.
**Domains:** Security, Firmware

### BPMP (Boot and Power Management Processor)
Falcon-based microcontroller responsible for early boot, power management, and sleep/resume (warmboot) firmware execution. First code to run after power-on reset.
**Domains:** Security, Firmware

### BVH (Bounding Volume Hierarchy)
Tree data structure used for ray tracing acceleration. The T239's RT Cores perform hardware-accelerated BVH traversal, processing two box node intersections per cycle.
**Domains:** GPU

### Cache Coherency
Protocol ensuring consistent data views across multiple caches. The T239 uses MESI (Modified, Exclusive, Shared, Invalid) protocol managed by the DSU's Snoop Control Unit for CPU cores.
**Domains:** CPU, Memory

### Capability-Based Security
Security model where processes interact with kernel objects exclusively through typed handles stored in per-process handle tables. Enforced by the Horizon microkernel.
**Domains:** Firmware

### CAS Latency (CL)
Column Access Strobe latency — the delay between a read command and data availability in DRAM. T239 LPDDR5X operates at CL 28–36 cycles at 6,400 MT/s.
**Domains:** Memory

### Command Buffer
HIPC message transmission mechanism — a 0x100-byte (256-byte) region in each thread's TLS area used for IPC message passing between Horizon OS processes.
**Domains:** Firmware

### Compute Capability
NVIDIA GPU architecture version identifier. The T239 GPU is compute capability 8.6 (SM86), indicating Ampere architecture with specific ISA and resource limits.
**Domains:** GPU

### Copy Engine (CE)
Dedicated GPU hardware for asynchronous data transfers between memory regions. Independent of SM execution units, allowing data movement to overlap with computation.
**Domains:** GPU, Memory

### CRC32
Cyclic redundancy check for data integrity verification. Hardware-accelerated via ARM Cryptographic Extensions with both Ethernet (CRC-32) and Castagnoli (CRC-32C) polynomial support.
**Domains:** CPU, Storage

### Cryptographic Extension
ARM processor extension adding hardware-accelerated AES, SHA-1, SHA-256, CRC32, and dot product instructions. Enabled on the T239's A78C cores.
**Domains:** CPU, Security

### CUDA Core
Basic FP32 execution unit in NVIDIA GPUs. Each SM86 SM contains 128 FP32 CUDA cores; the T239 has 1,536 total (12 SMs × 128).
**Domains:** GPU

### DMB (Data Memory Barrier)
ARM instruction ensuring memory access ordering. Required before/after DMA transfers to maintain data consistency between CPU and DMA engines.
**Domains:** CPU, Memory

### DMA (Direct Memory Access)
Data transfer mechanism that moves data between memory regions without CPU intervention. The T239 has multiple DMA engines: GPC-DMA, GPU Copy Engines, and Video DMA.
**Domains:** Memory, Storage, GPU

### DLSS (Deep Learning Super Sampling)
NVIDIA AI-powered rendering technology using Tensor Cores for temporal accumulation, feature extraction, super-resolution upscaling, and frame generation.
**Domains:** GPU

### Domain Object
HIPC multiplexing mechanism allowing multiple server-side objects to be accessed through a single session handle using domain IDs. Essential for services managing many open handles.
**Domains:** Firmware

### DRAM (Dynamic Random-Access Memory)
Volatile memory technology used in the T239's 12 GB LPDDR5X unified memory pool. Requires periodic refresh to maintain data integrity.
**Domains:** Memory

### DRM (Digital Rights Management)
Multi-layered content protection spanning hardware (TSEC, TrustZone, eFuses), firmware (secure boot), and software (Denuvo anti-tamper) to prevent piracy and tampering.
**Domains:** Security, Firmware

### DSU (DynamIQ Shared Unit)
ARM cluster interconnect connecting all 8 A78C cores to the shared L3 cache. Manages coherency via the Snoop Control Unit (SCU) and provides external memory interfaces.
**Domains:** CPU, Memory

### DVFS (Dynamic Voltage and Frequency Scaling)
Power management technique that dynamically adjusts voltage and frequency based on workload. Applied to CPU cores, GPU, and memory controller.
**Domains:** CPU, Memory, GPU

### eFuse
One-Time Programmable (OTP) fuse array on the T239 die for permanent security-critical storage. Stores root keys, device identity, anti-rollback counters, and HDCP keys.
**Domains:** Security

### EL0–EL3 (Exception Levels)
ARM privilege levels: EL0 (application), EL1 (OS kernel), EL2 (hypervisor), EL3 (secure monitor). Higher levels have greater privilege; Switch 2 games run at EL0.
**Domains:** CPU, Security, Firmware

### eMMC (Embedded MultiMediaCard)
Legacy flash storage standard used in Switch 1 (eMMC 5.1, ~100-200 MB/s). Replaced by UFS 3.1 in Switch 2.
**Domains:** Storage

### ExeFS (Executable Filesystem)
PFS0-format filesystem within NCA sections containing game executables: code.bin, rtld, sdk, subsdk files, icon, and npdm metadata.
**Domains:** Storage, Firmware

### Falcon
NVIDIA's proprietary RISC microprocessor architecture used in TSEC and MTS security co-processors. Not an ARM core; has its own instruction set and security modes.
**Domains:** Security, Firmware

### FDE (File Decompression Engine)
Hardware accelerator on the T239 performing LZ4 decompression in fixed-function silicon, offloading the CPU during asset loading and streaming.
**Domains:** Storage, GPU

### Firmware Ratchet
Anti-rollback mechanism using eFuse counters that are monotonically incremented with security-critical updates. Once burned, cannot be decremented.
**Domains:** Security, Firmware

### FP32/FP64
Single-precision (32-bit) and double-precision (64-bit) floating-point formats. SM86 has 128 FP32 cores and 4 configurable FP64 cores per SM.
**Domains:** GPU

### Fused Multiply-Add (FMA/FMUL/FADD)
Combined multiply-and-add arithmetic operation executed in a single cycle. Primary arithmetic operation on both GPU CUDA cores and CPU FP pipelines.
**Domains:** GPU, CPU

### Game Card (XCI)
Proprietary read-only cartridge storing game content in NCA format on internal NAND flash. Provides physical distribution and certificate-based authentication.
**Domains:** Storage, Security

### GameChat
Switch 2 feature enabling voice and video chat between players during online multiplayer. Uses hardware Opus codec and dedicated audio/video processing.
**Domains:** Display/IO, Firmware

### GIC (Generic Interrupt Controller)
ARM GICv3/v4 interrupt controller managing interrupt routing to CPU cores. Uses system register-based CPU interface (ICC_*_ELx) instead of memory-mapped registers.
**Domains:** CPU

### GPC (Graphics Processing Cluster)
Top-level GPU organizational unit. The T239 uses a single GPC containing all 6 TPCs (12 SMs), unlike desktop Ampere which typically has multiple GPCs.
**Domains:** GPU

### GPC-DMA
General Purpose Copy DMA engine for memory-to-memory copies, scatter-gather operations, and peripheral DMA. Registers at base 0x0261_0000.
**Domains:** Memory

### GPUVA (GPU Virtual Address)
GPU virtual address space used for bindless resource access in NVN2 Tier 3 binding.
**Domains:** GPU, Firmware

### Handle
Typed reference to a kernel object stored in a per-process handle table in Horizon OS. Handles are process-local; cross-process sharing requires explicit IPC transfer.
**Domains:** Firmware

### HDCP (High-bandwidth Digital Content Protection)
Encryption standard (HDCP 2.3) protecting HDMI video output. Keys stored in eFuses, accessible only through TSEC Heavy Secure firmware.
**Domains:** Security, Display/IO

### HIPC (Horizon IPC)
Inter-process communication protocol for Horizon OS. Every system service interaction uses HIPC messages transmitted through command buffers with zero-copy buffer descriptors.
**Domains:** Firmware

### Horizon OS
Nintendo's proprietary microkernel-based real-time operating system running on Switch consoles. L4-derived design with capability-based security and userland services.
**Domains:** Firmware

### HPB (Host Performance Booster)
UFS 3.1 feature caching the flash translation table (L2P mapping) in host DRAM, reducing random read latency by 20–40%.
**Domains:** Storage

### HRTF (Head-Related Transfer Function)
Audio processing technique for 3D spatial audio rendering. Converts surround sources to binaural stereo for headphone playback.
**Domains:** Display/IO

### IBB (Initial Boot Blob)
First executable bootloader code (MB1/MB2 in NVIDIA nomenclature) verified by the BootROM. Initializes DRAM, configures security engines, and establishes TrustZone.
**Domains:** Security, Firmware

### INI1 (Initial Processes Image 1)
Archive format containing KIP (Kernel Initial Process) images loaded by the kernel during boot. Parsed to create address spaces for each initial process.
**Domains:** Firmware

### IOMMU (Input/Output Memory Management Unit)
Hardware unit translating IOVA (I/O Virtual Address) to physical addresses for DMA devices. Provides memory protection and isolation for device-initiated transfers.
**Domains:** Memory

### IPC (Inter-Process Communication)
Mechanism for processes to exchange data and synchronize. In Horizon OS, all IPC uses the HIPC protocol with kernel-mediated message passing.
**Domains:** Firmware

### JEDEC
Industry standards organization defining DRAM (LPDDR5X: JESD209-5) and storage (UFS 3.1: JESD220E) specifications used by the T239.
**Domains:** Memory, Storage

### Joy-Con 2
Switch 2 detachable wireless controllers communicating via Bluetooth Low Energy with capacitive analog sticks, NFC (right controller), and IR camera.
**Domains:** Display/IO

### KFUSE (Key Fuse Interface)
Hardware interface connecting the SCP's CTL block to the eFuse array for secure key retrieval during early boot.
**Domains:** Security

### KIP (Kernel Initial Process)
First userland processes started by the Horizon kernel during boot. Embedded in INI1 archive; include loader, pm, sm, fs_mitm, spl, and others.
**Domains:** Firmware

### L1/L2/L3 Cache
Multi-level cache hierarchy: L1 (per-core, 64 KB instruction + 64 KB data), L2 (per-core, 256 KB), L3 (shared, 4 MB via DSU). Total ~7 MB SRAM on T239.
**Domains:** CPU, Memory

### LPDDR5X
Low-Power Double Data Rate 5X memory (JEDEC JESD209-5). The T239 uses 12 GB in two 6 GB modules at 6,400 MT/s (docked) or 4,200 MT/s (handheld).
**Domains:** Memory

### LZ4
Lossless compression algorithm optimized for decompression speed. Used by the T239's FDE hardware engine for game asset decompression during loading.
**Domains:** Storage

### MAIR (Memory Attribute Indirection Register)
AArch64 system register defining memory type encodings for 8 attribute indirection registers (Device, Normal NC, Write-Through, Write-Back).
**Domains:** CPU, Memory

### MESI Protocol
Cache coherency protocol with four states: Modified, Exclusive, Shared, Invalid. Managed by the DSU's Snoop Control Unit for CPU cache coherency.
**Domains:** CPU, Memory

### Microkernel
OS kernel design philosophy where only essential primitives (scheduling, VM, IPC, sync) run in kernel space; all other services run in userland processes.
**Domains:** Firmware

### MMA (Matrix Multiply-Accumulate)
Tensor Core operation performing warp-level matrix multiplication. SM86 supports HMMA (FP16), IMMA (INT8), DMMA (FP64), and BMMA (INT1) formats.
**Domains:** GPU

### MMIO (Memory-Mapped I/O)
Hardware register access mechanism where device registers are mapped into the physical address space. Used for GPU, display controller, storage, and peripheral control.
**Domains:** Memory, GPU, Display/IO, Storage

### MMU (Memory Management Unit)
Hardware unit performing virtual-to-physical address translation. Each A78C core has an MMU supporting 48-bit virtual/physical address space with 4 KB/16 KB/64 KB pages.
**Domains:** CPU, Memory

### MTE (Memory Tagging Extension)
ARMv8.5-A feature assigning 4-bit tags to 16-byte memory granules and pointers for hardware-assisted detection of memory safety violations.
**Domains:** Security, CPU

### MTS (Microcontroller for Task Scheduling)
Falcon-based microcontroller responsible for GPU task scheduling and context switching. Isolated from the game-visible CPU environment.
**Domains:** Security, Firmware, GPU

### NCA (Nintendo Content Archive)
Container format for all distributable Switch 2 content. Contains RSA-2048 signature, encrypted key area, section entries, and content sections (PFS0/RomFS).
**Domains:** Storage, Security, Firmware

### NEON
See ASIMD. ARM's 128-bit SIMD extension for parallel data processing on integer and floating-point vectors.
**Domains:** CPU

### NSP (Nintendo Submission Package)
Higher-level container bundling one or more NCAs together for distribution. Analogous to an installer package.
**Domains:** Storage

### NVENC/NVDEC
Hardware video encoder (NVENC) and decoder (NVDEC) in the T239. Supports H.264, H.265, and AV1 encode/decode at up to 4K@60fps.
**Domains:** GPU, Display/IO, Memory

### NVN2
Nintendo's proprietary graphics API for Switch 2. Thin abstraction over the NVIDIA GPU driver (nvdrv service) with low-level GPU command submission.
**Domains:** GPU, Firmware

### Occupancy
Ratio of active warps to maximum warps per SM. SM86 supports up to 48 warps (1,536 threads) per SM; occupancy is limited by register and shared memory usage.
**Domains:** GPU

### OFA (Optical Flow Accelerator)
Dedicated hardware unit computing dense optical flow between frames. Primary enabler for DLSS 3 frame generation.
**Domains:** GPU

### OP-TEE (Open Portable Trusted Execution Environment)
Trusted OS running at S-EL1 in ARM TrustZone Secure World. Provides kernel for trusted applications, secure storage, and cryptographic operations.
**Domains:** Security

### PAC (Pointer Authentication Codes)
ARMv8.3-A extension cryptographically signing pointers to detect tampering. Uses QARMA block cipher with 5 independent key types (APIA, APIB, APDA, APDB, APGA).
**Domains:** Security, CPU

### Package1/Package2
Firmware payload containers in the Switch boot chain. Package1 (MB1) handles DRAM init and TSEC setup; Package2 contains the kernel, INI1, and warmboot firmware.
**Domains:** Firmware, Security

### PCIe (Peripheral Component Interconnect Express)
High-speed serial bus standard. Used by SD Express cards (PCIe Gen3 x1) for storage and by the dock for USB-C DisplayPort Alt Mode.
**Domains:** Storage, Display/IO

### Pipeline (GPU)
Functional execution unit in the SM. SM86 has 9 pipelines: int, fmalighter, fp16, fma64lite, mio, cbu, udp, ttu, and fe.
**Domains:** GPU

### Pipeline (CPU)
Instruction execution stages. The A78C has a 13-stage integer pipeline with 4-wide decode, 6-wide rename, and 13 execution ports.
**Domains:** CPU

### PFS0 (Partition File System 0)
Simple flat filesystem used for ExeFS sections in NCA containers. No directory hierarchy; flat collection of named files.
**Domains:** Storage

### PoP (Package-on-Package)
IC packaging technique where DRAM is mounted on top of the SoC die. Likely used for the T239's LPDDR5X modules.
**Domains:** Memory

### Predication
SM86 feature where all instructions support conditional execution via a 3-bit predicate field. The special PT (P7) register is always true.
**Domains:** GPU

### Priority Inheritance
Synchronization mechanism where a low-priority thread holding a mutex is temporarily boosted to the priority of a high-priority waiter, preventing priority inversion.
**Domains:** Firmware

### Process (Horizon OS)
Kernel object representing an address space, handle table, and set of capabilities. Each system service and game runs in its own process.
**Domains:** Firmware

### QoS (Quality of Service)
Priority-based arbitration in the memory controller allocating bandwidth among competing masters (CPU, GPU, display, video, DMA) with latency and throughput guarantees.
**Domains:** Memory

### Register File
On-chip storage for operand values. SM86 has 65,536 × 32-bit registers per SM (256 KB). A78C has ~160 physical integer and ~160 FP/SIMD registers per core.
**Domains:** GPU, CPU

### Ring Buffer
Circular buffer used for GPU command submission in NVN2. Commands are written to the ring by the CPU and consumed by the GPU hardware.
**Domains:** GPU, Firmware

### RomFS (Read-Only Filesystem)
Hierarchical read-only filesystem used in NCA sections for game asset storage. Supports random access by offset with hash-based integrity verification.
**Domains:** Storage

### RT Core
Dedicated ray tracing hardware in each SM. 2nd-generation (Ampere) with BVH traversal, ray-box/ray-triangle intersection, and opacity micromap acceleration.
**Domains:** GPU

### SASS (Shader Assembly)
Low-level instruction set executed by NVIDIA GPUs. SM86 uses 128-bit (16-byte) instruction words with 1,271 instruction variants across all functional pipelines.
**Domains:** GPU

### SCU (Snoop Control Unit)
Hardware unit in the DSU maintaining cache coherency across all 8 A78C cores. Snoops L2 caches and provides coherency with GPU coherent accesses.
**Domains:** CPU, Memory

### Scoreboard
Dependency tracking mechanism in SM86 that tracks 6 outstanding memory/dependency slots per warp. DEPBAR instructions explicitly manage scoreboard dependencies.
**Domains:** GPU

### SCP (Secure Co-Processor)
Security-critical component within TSEC. Manages the eFuse interface (KFUSE), performs key derivation, and provides hidden MMIO registers for key material.
**Domains:** Security

### SD Express
Next-generation SD card standard combining traditional SD interface with PCIe Gen3 x1 and NVMe protocol. Up to 985 MB/s sequential read.
**Domains:** Storage

### Secure Boot Chain
Multi-stage boot verification where each stage cryptographically verifies the next. Anchored in the on-die BootROM with eFuse-stored public key hashes.
**Domains:** Security, Firmware

### SEMI-Custom SoC
System-on-Chip design where NVIDIA customizes a base architecture (Ampere/ARM) for a specific customer (Nintendo). The T239 is not a standard off-the-shelf part.
**Domains:** GPU, CPU

### SER (Shader Execution Reordering)
Ada architecture feature reordering shader threads after ray tracing dispatch to reduce execution divergence and improve Tensor Core utilization.
**Domains:** GPU

### Session (HIPC)
Kernel object representing an IPC connection endpoint between client and server processes. Created by the Service Manager during service discovery.
**Domains:** Firmware

### SHA-256
Cryptographic hash algorithm producing 256-bit digests. Used for eFuse PKC hash, boot signature verification, and integrity checking throughout the security architecture.
**Domains:** Security, CPU

### Shared Memory (GPU)
Programmable on-chip memory shared among threads in a thread block. SM86 provides 100 KB per SM, configurable between shared memory and L1 cache.
**Domains:** GPU

### SM (Streaming Multiprocessor)
Basic GPU execution unit containing CUDA cores, Tensor Cores, RT Core, warp schedulers, register file, and shared memory. T239 has 12 SMs.
**Domains:** GPU

### SM86
NVIDIA GPU architecture variant for the T239. Compute capability 8.6 with specific resource limits: 48 max warps/SM, 100 KB shared memory/SM, 65,536 registers/SM.
**Domains:** GPU

### SMC (Secure Monitor Call)
ARM instruction for Normal World software to request services from Secure World. Used for world switching, key derivation, and DRM operations.
**Domains:** Security, CPU, Firmware

### SPIR-V
Intermediate shader representation used in the NVN2 compilation pipeline. Games compile shaders to SPIR-V, which the driver then translates to SASS.
**Domains:** GPU, Firmware

### TSEC (Tegra Security Co-processor)
Dedicated security processor based on NVIDIA Falcon architecture. Handles HDCP key management, key derivation, and DRM enforcement in Heavy Secure mode.
**Domains:** Security, Firmware

### TLS (Thread-Local Storage)
Per-thread memory region (0x200 bytes) at a fixed address in the process's virtual address space. Contains the HIPC command buffer and per-thread state.
**Domains:** Firmware

### TPC (Texture Processing Cluster)
GPU organizational unit containing 2 SMs. The T239 has 6 TPCs in a single GPC.
**Domains:** GPU

### TrustZone
ARM hardware security technology partitioning the processor into Secure World and Normal World. Every bus transaction is tagged as secure or non-secure.
**Domains:** Security, CPU, Firmware

### UFS (Universal Flash Storage)
JEDEC-defined flash storage standard (JESD220E) using MIPI M-PHY/UniPro. The T239 uses UFS 3.1 with 256 GB capacity and ~2.1 GB/s sequential reads.
**Domains:** Storage

### UMA (Unified Memory Architecture)
Memory design where CPU and GPU share a single physical memory pool. Eliminates PCIe transfer overhead but creates bandwidth contention.
**Domains:** Memory, GPU

### VRR (Variable Refresh Rate)
Display technology synchronizing refresh cycle with GPU frame output, eliminating screen tearing without V-Sync input lag. Supported on internal LCD and external displays.
**Domains:** Display/IO, GPU

### Warp
Group of 32 GPU threads that execute in lockstep. The fundamental unit of execution on NVIDIA GPUs. SM86 supports up to 48 concurrent warps per SM.
**Domains:** GPU

### Warp Scheduler
SM hardware that selects warps for execution each cycle. SM86 has 2 warp schedulers per SM, each issuing one instruction to two sub-partitions per cycle.
**Domains:** GPU

### Warmboot
Sleep/resume firmware path that preserves DRAM contents during power-down. Runs on the BPMP; skips DRAM re-initialization on resume.
**Domains:** Firmware, Memory

### W^X (Write XOR Execute)
Memory protection policy preventing pages from being simultaneously writable and executable. Defeats code injection attacks.
**Domains:** Security, CPU

### XCI (GameCard Image)
Container format for game card content. Contains XCI header, encrypted NCA data, and game card certificate for authentication.
**Domains:** Storage, Security

---

## Confidence Legend

Terms marked **CONFIRMED** are verified from official documentation, silicon analysis, or oboromi source code. Terms marked **INFERRED** are derived from closely related public documentation. Terms marked **SPECULATIVE** are based on industry analysis or extrapolation.

---

*Generated from docs/gpu.md, docs/cpu.md, docs/memory.md, docs/security.md, docs/firmware.md, docs/display-io.md, docs/storage.md*
