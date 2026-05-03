# GPU Architecture Reference: NVIDIA T239 (SM86 Ampere)

> **Target:** Nintendo Switch 2 SoC — NVIDIA T239 custom processor
> **Compute Capability:** 8.6 (sm_86)
> **Architecture Family:** Ampere (with Ada hybrid features)
> **Document Status:** Work in progress — sections 1–5 of planned full coverage
>
> **Confidence Legend:**
> - **CONFIRMED** — Verified from NVIDIA official documentation, silicon, or oboromi source code
> - **INFERRED** — Derived from closely related public documentation (Orin TRM, Ampere whitepapers)
> - **SPECULATIVE** — Based on industry analysis, reverse engineering, or extrapolation

---

## Table of Contents

1. [GPU Architecture Overview](#1-gpu-architecture-overview)
2. [SM/TPC Topology](#2-smtpc-topology)
3. [Execution Units](#3-execution-units)
4. [Register File Layout](#4-register-file-layout)
5. [SASS ISA Reference](#5-sass-isa-reference)

---

## 1. GPU Architecture Overview

### 1.1 T239 SoC Summary

The T239 is a custom system-on-chip designed by NVIDIA for the Nintendo Switch 2 console.
It combines ARM CPU cores with an Ampere-based GPU on a single die. [1][2]

```
+------------------------------------------------------------------+
|                        NVIDIA T239 SoC                           |
|                                                                  |
|  +-------------------+    +----------------------------------+   |
|  |    CPU Complex     |    |         GPU (Ampere SM86)        |   |
|  |                    |    |                                  |   |
|  |  8x ARM Cortex-A78C|    |  12 SMs in 6 TPCs (1 GPC)      |   |
|  |  6 user + 2 system |    |  1,536 CUDA cores               |   |
|  |  64KB L1I + L1D/ea |    |  48 Tensor Cores                |   |
|  |                    |    |  12 RT Cores                     |   |
|  +-------------------+    +----------------------------------+   |
|                                                                  |
|  +----------------------------------------------------------+   |
|  |              Memory Interface (128-bit LPDDR5X)          |   |
|  |              12 GB total (9 GB for games)                 |   |
|  +----------------------------------------------------------+   |
+------------------------------------------------------------------+
```

**Figure 1.1:** T239 SoC block diagram (ASCII). CPU and GPU share the unified
memory subsystem via a 128-bit LPDDR5X interface. [1][3]

### 1.2 Key Specifications

| Parameter | Docked Mode | Handheld Mode | Max (Developer Override) |
|---|---|---|---|
| GPU Clock | 1,007 MHz [1] | 561 MHz [1] | ~1,400 MHz [SPECULATIVE] |
| FP32 TFLOPS | ~3.07 [3] | ~1.71 [3] | ~4.3 [SPECULATIVE] |
| Memory Bandwidth | 102 GB/s [1] | 68 GB/s [1] | — |
| CPU Clock | 998 MHz [1] | 1,101 MHz [1] | 1,700 MHz [1] |

**Table 1.1:** T239 performance characteristics by power mode. TFLOPS calculated
as `cores × 2 × clock_Hz` for fused multiply-add throughput. [1][3]

### 1.3 Manufacturing Process

The T239 is fabricated on a Samsung process node identified as a hybrid of 8nm and
10nm lithography. [SPECULATIVE — Digital Foundry analysis suggests Samsung 8LPE or
similar, but no official confirmation exists.] [1]

### 1.4 GPU Feature Set

| Feature | Support | Notes |
|---|---|---|
| CUDA Cores | 1,536 | 12 SMs × 128 FP32 cores per SM [CONFIRMED] |
| RT Cores | 12 | 1 per SM, 2nd generation (Ampere) [CONFIRMED] |
| Tensor Cores | 48 | 4 per SM, 3rd generation (Ampere) [CONFIRMED] |
| DLSS | Yes | DLSS 1x/2x/3x modes + DLAA [CONFIRMED] [1] |
| Ray Tracing | Yes | HW-accelerated BVH traversal [CONFIRMED] |
| FP64 | Configurable | Present in SM86 ISA; ratio TBC for T239 [INFERRED] |
| NVN2 API | Yes | Nintendo proprietary graphics API [CONFIRMED] |
| Vulkan | Partial | Driver support exists for development [INFERRED] |

**Table 1.2:** T239 GPU feature matrix. [1][3]

### 1.5 Memory Hierarchy

```
+------------------+     +------------------+     +------------------+
| Register File    |     | Shared Memory    |     | L1 Cache         |
| 64K 32-bit/SM    |<--->| 100 KB/SM        |<--->| Combined w/SMEM  |
| 255 regs/thread  |     | (configurable)   |     | 128 KB total/SM  |
+------------------+     +------------------+     +------------------+
                                                           |
                                                           v
                                                  +------------------+
                                                  | L2 Cache         |
                                                  | 4 MB unified     |
                                                  +------------------+
                                                           |
                                                           v
                                                  +------------------+
                                                  | DRAM             |
                                                  | 12 GB LPDDR5X   |
                                                  | 128-bit bus      |
                                                  +------------------+
```

**Figure 1.2:** Memory hierarchy (ASCII). The register file, shared memory, and
L1 cache are per-SM resources. The L2 cache is unified across all SMs. [2][4]

---

## 2. SM/TPC Topology

### 2.1 SM Organization

The T239 GPU contains **12 Streaming Multiprocessors (SMs)** organized into
**6 Texture Processing Clusters (TPCs)**, each containing **2 SMs**. [INFERRED
from 1,536 cores ÷ 128 cores/SM = 12 SMs; 12 SMs ÷ 2 SMs/TPC = 6 TPCs.] [1][2]

```
+------------------------------------------------------------------+
|                    GPU (12 SMs / 6 TPCs)                         |
|                                                                  |
|  +--GPC 0--------------------------------------------------------+
|  |                                                               |
|  |  +--TPC 0---+  +--TPC 1---+  +--TPC 2---+                    |
|  |  | SM0 | SM1|  | SM2 | SM3|  | SM4 | SM5|                    |
|  |  +---------+  +---------+  +---------+                        |
|  |                                                               |
|  |  +--TPC 3---+  +--TPC 4---+  +--TPC 5---+                    |
|  |  | SM6 | SM7|  | SM8 | SM9|  | SM10| SM11|                   |
|  |  +---------+  +---------+  +---------+                        |
|  |                                                               |
|  +---------------------------------------------------------------+
|                                                                  |
|  +--L2 Cache (4 MB)--------------------------------------------+
|  +---------------------------------------------------------------+
+------------------------------------------------------------------+
```

**Figure 2.1:** SM/TPC topology. The T239 uses a single Graphics Processing
Cluster (GPC) containing all 6 TPCs. This is a departure from desktop Ampere
parts which typically have multiple GPCs. [INFERRED — single GPC is consistent
with mobile SoC designs and Orin TRM documentation.] [2]

### 2.2 SM Sub-Partition Structure

Each SM is divided into **4 processing blocks** (sub-partitions), consistent
with the Ampere SM86 architecture. [CONFIRMED from NVIDIA Ampere tuning guide
and oboromi decoder architecture.] [2][4]

```
+--SM----------------------------------------------------------------+
|                                                                     |
|  +--Sub-partition 0--+ +--Sub-partition 1--+                        |
|  | Warp Scheduler 0  | | Warp Scheduler 1  |                        |
|  | 32 FP32 cores     | | 32 FP32 cores     |                        |
|  | 16 INT32 cores    | | 16 INT32 cores    |                        |
|  | 1 SFU             | | 1 SFU             |                        |
|  | 1 LD/ST unit      | | 1 LD/ST unit      |                        |
|  | 1 Tensor Core     | | 1 Tensor Core     |                        |
|  +-------------------+ +-------------------+                        |
|                                                                     |
|  +--Sub-partition 2--+ +--Sub-partition 3--+                        |
|  | Warp Scheduler 2  | | Warp Scheduler 3  |                        |
|  | 32 FP32 cores     | | 32 FP32 cores     |                        |
|  | 16 INT32 cores    | | 16 INT32 cores    |                        |
|  | 1 SFU             | | 1 SFU             |                        |
|  | 1 LD/ST unit      | | 1 LD/ST unit      |                        |
|  | 1 Tensor Core     | | 1 Tensor Core     |                        |
|  +-------------------+ +-------------------+                        |
|                                                                     |
|  +--Shared Resources---------------------------------------------+ |
|  |  Shared Memory / L1 Data Cache: 100 KB (configurable)         | |
|  |  Register File: 65,536 × 32-bit                              | |
|  |  1 RT Core (shared across all sub-partitions)                 | |
|  |  Warp schedulers: 2 (select from 4 sub-partition queues)      | |
|  +---------------------------------------------------------------+ |
+---------------------------------------------------------------------+
```

**Figure 2.2:** SM internal structure. Two warp schedulers select from four
sub-partition warp queues each cycle, issuing up to one instruction per
sub-partition per cycle. [2][4]

### 2.3 Warp Scheduling

SM86 has **2 warp schedulers per SM** that select from **4 sub-partition
warp queues**. Each scheduler can issue one independent instruction per cycle
to two sub-partitions. The maximum concurrent warp count is **48 per SM** for
compute capability 8.6. [CONFIRMED from NVIDIA Ampere tuning guide.] [2]

| Resource | SM86 (cc 8.6) | SM80 (cc 8.0, A100) |
|---|---|---|
| Max warps/SM | 48 [2] | 64 [2] |
| Max thread blocks/SM | 16 [2] | 32 [2] |
| Shared memory/SM | 100 KB [2] | 164 KB [2] |
| Max shared memory/block | 99 KB [2] | 163 KB [2] |
| Registers/SM | 65,536 [2] | 65,536 [2] |

**Table 2.1:** SM86 vs SM80 resource comparison. The SM86 configuration trades
warp capacity for higher per-warp resources in a more compact design. [2]

### 2.4 Shared Memory and L1 Cache

The **100 KB per SM** on SM86 is partitioned between shared memory (programmable)
and L1 data cache (hardware-managed). The default split is configurable via the
CUDA API or driver settings. [CONFIRMED] [2]

The combined shared memory + L1 capacity on SM86 is **128 KB** total per SM,
with 100 KB available for shared memory allocation and the remainder serving
as L1 tag storage and other overhead. [INFERRED from Ampere tuning guide.] [2]

---

## 3. Execution Units

### 3.1 Execution Unit Inventory (per SM)

| Unit | Count/SM | Description |
|---|---|---|
| FP32 CUDA Cores | 128 | Single-precision floating-point (FADD, FMUL, FFMA) [CONFIRMED] |
| INT32 Cores | 64 | 32-bit integer (IADD, IMUL, IMAD) [CONFIRMED] |
| FP64 Cores | 4 (configurable) | Double-precision (DADD, DMUL, DFMA) [INFERRED] |
| Load/Store Units | 4 | One per sub-partition [CONFIRMED] |
| Special Function Units | 4 | SFU: SIN, COS, RCP, RSQ, SQRT, EX2, LG2, TANH [CONFIRMED] |
| Tensor Cores | 4 | Matrix multiply-accumulate (HMMA, IMMA, DMMA) [CONFIRMED] |
| RT Core | 1 | Ray tracing: BVH traversal, ray-box/ray-triangle [CONFIRMED] |
| Texture Units | 4 | TEX, TLD, TLD4, TXQ operations [CONFIRMED] |
| Warp Scheduler | 2 | Select from 4 sub-partition queues [CONFIRMED] |

**Table 3.1:** SM86 execution unit inventory. The 1,536 total CUDA cores come
from 12 SMs × 128 FP32 cores/SM. [1][2][4]

### 3.2 Execution Pipeline Architecture

The SM86 instruction set is organized into functional pipelines. Each pipeline
handles a specific class of operations with defined latency and throughput
characteristics. [CONFIRMED from oboromi `sm_86_latencies.txt` and NVIDIA
documentation.] [4][5]

```
+--SM86 Execution Pipelines------------------------------------------+
|                                                                     |
|  int_pipe      — FP32/INT32 ALU, logic, shifts, conversions        |
|  fmalighter_pipe — FP32 FMA, FADD, FMUL, IMAD, IMUL               |
|  fp16_pipe     — FP16 ALU (HADD2, HFMA2, HMUL2, HMMA)             |
|  fma64lite_pipe — FP64 (DFMA, DADD, DMUL, DMMA, CLMAD)            |
|  mio_pipe      — Memory I/O, texture, shared memory, barriers      |
|  cbu_pipe      — Control flow (BRA, CALL, RET, EXIT, BSYNC)        |
|  udp_pipe      — Uniform data processing (UIADD3, ULOP3, UMOV)     |
|  ttu_pipe      — Tensor core operations                            |
|  fe_pipe       — Front-end (DEPBAR, NOP, PMTRIG)                   |
+---------------------------------------------------------------------+
```

**Figure 3.1:** SM86 execution pipeline map. Derived from the operation sets
defined in the SM86 ISA specification. [4][5]

### 3.3 Instruction Latency Reference

The following table shows representative instruction latencies (in clock cycles)
for SM86, derived from the latency model in `sm_86_latencies.txt`. [CONFIRMED
from oboromi source.] [5]

| Operation Class | Latency (cycles) | Throughput | Notes |
|---|---|---|---|
| FP32 FADD/FMUL | 4–6 | 1/cycle/sub-partition | Via fmalighter_pipe [5] |
| FP32 FFMA | 5–6 | 1/cycle/sub-partition | Via fmalighter_pipe [5] |
| INT32 IADD3 | 6 | 1/cycle/sub-partition | Via int_pipe [5] |
| INT32 IMAD | 5–6 | 1/cycle/sub-partition | Via fmalighter_pipe [5] |
| FP64 DFMA/DADD/DMUL | 10–12 | 1 per 2 cycles | Via fma64lite_pipe [5] |
| FP16 HADD2/HFMA2/HMUL2 | 5 | 1/cycle/sub-partition | Via fp16_pipe [5] |
| HMMA (Tensor) | 22–27 | 1 per 8 cycles | Via fp16_pipe, 8-wide output [5] |
| IMMA (Tensor) | 22–27 | 1 per 8 cycles | Via int_pipe, 8-wide output [5] |
| DMMA (Tensor) | 20–25 | 1 per 8 cycles | Via fma64lite_pipe [5] |
| LDG (global load) | ~200+ (off-chip) | Variable | Via mio_pipe, latency hiding required [5] |
| LDS (shared load) | ~20–30 | 1/cycle/sub-partition | Via mio_pipe [5] |
| STS (shared store) | ~20–30 | 1/cycle/sub-partition | Via mio_pipe [5] |
| BRA/BRX (branch) | 0 (predict) | 1/cycle | Via cbu_pipe, 0-delay branching [5] |
| DEPBAR | 0 | 1/cycle | Via fe_pipe, scoreboard management [5] |
| LOP3 (3-input logic) | 6 | 1/cycle/sub-partition | Via int_pipe [5] |
| SHF (funnel shift) | 6 | 1/cycle/sub-partition | Via int_pipe [5] |
| MUFU (SFU) | Variable | 1 per 4 cycles | SIN, COS, RCP, RSQ, SQRT, EX2, LG2, TANH [5] |

**Table 3.2:** SM86 instruction latency summary. Latencies are approximate and
vary by operand type, bank conflicts, and pipeline contention. [5]

### 3.4 Tensor Core Capabilities (3rd Gen Ampere)

The SM86 Tensor Cores support matrix multiply-accumulate operations in multiple
precision formats. Each Tensor Core operates on warp-level matrix fragments.
[CONFIRMED from ISA spec and NVIDIA documentation.] [2][4]

| MMA Operation | Input Types | Output Type | Matrix Sizes |
|---|---|---|---|
| HMMA | FP16, BF16 | FP16, FP32 | 16×8×16, 16×8×8 [4] |
| IMMA | INT8, INT4, INT1 | INT32 | 16×8×16, 16×8×32 [4] |
| DMMA | FP64 | FP64 | 8×8×4 [4] |
| BMMA | INT1 | INT32 | 16×8×256 [4] |
| CLMAD | FP16 | FP32 | Cross-lane multiply-add [4] |

**Table 3.3:** Tensor Core MMA operation matrix. The 3rd-gen Tensor Cores in
SM86 add INT4/INT1 support and improved FP16 throughput over Turing. [2][4]

### 3.5 RT Core Capabilities (2nd Gen Ampere)

Each SM contains one 2nd-generation RT Core that accelerates:
- **BVH traversal** — hardware-accelerated bounding volume hierarchy walking
- **Ray-box intersection** — axis-aligned bounding box tests
- **Ray-triangle intersection** — Möller-Trumbore with Watertight mode
- **Opacity micromap** — hardware alpha testing for transparency
- **Motion blur** — ray-triangle tests on moving geometry [INFERRED from Ampere
  RT core documentation; exact T239 RT core features may differ.] [2]

---

## 4. Register File Layout

### 4.1 Register File Overview

The SM86 register file is a **65,536 × 32-bit** (256 KB) unified register file
shared among all active threads on an SM. Each thread can access up to **255
32-bit registers** (R0–R253, plus the zero register RZ=255). [CONFIRMED from
NVIDIA documentation and oboromi `sm86.rs` source: `MAX_REG_COUNT = 254` with
RZ at index 255.] [2][4]

### 4.2 Register File Allocation Diagram

```
+--Register File (65,536 × 32-bit per SM)----------------------------+
|                                                                     |
|  +--Thread Registers (R0–R253, RZ)--------------------------------+|
|  |  Allocated per-thread from the shared pool                     ||
|  |  Max 255 registers per thread                                  ||
|  |  Occupancy = 65536 / (threads_per_SM × regs_per_thread)       ||
|  |  Example: 255 regs/thread → 65536/255 = 257 threads           ||
|  |           128 regs/thread → 65536/128 = 512 threads           ||
|  |           64 regs/thread  → 65536/64 = 1024 threads           ||
|  +----------------------------------------------------------------+|
|                                                                     |
|  +--Predicate Registers (P0–P6, PT)-------------------------------+|
|  |  8 predicate registers per thread                              ||
|  |  P0–P6: General-purpose predicate bits                         ||
|  |  PT: Always-true (read-only, cannot be modified)               ||
|  |  Stored as bits within a single u8 per thread                  ||
|  +----------------------------------------------------------------+|
|                                                                     |
|  +--Uniform Registers (UR0–UR62, URZ)----------------------------+|
|  |  63 uniform registers per warp (shared across all 32 threads)  ||
|  |  URZ (UR63): Always zero                                       ||
|  |  Used for warp-uniform values: constants, addresses            ||
|  +----------------------------------------------------------------+|
|                                                                     |
|  +--Uniform Predicates (UP0–UP6, UPT)----------------------------+|
|  |  8 uniform predicate registers per warp                        ||
|  |  UPT: Always-true                                              ||
|  |  Used for warp-uniform control flow decisions                  ||
|  +----------------------------------------------------------------+|
|                                                                     |
|  +--Special Registers (SR0–SR255)---------------------------------+|
|  |  Read-only hardware state registers                            ||
|  |  SR_LANEID: Lane ID within warp (0–31)                         ||
|  |  SR_CLOCK/SR_CLOCKLO/SR_CLOCKHI: Clock counters                ||
|  |  SR_TID.X/Y/Z: Thread ID within block                          ||
|  |  SR_CTAID.X/Y/Z: Block ID within grid                          ||
|  |  SR_NTID: Block dimensions                                     ||
|  |  SR_EQMASK/LTMASK/LEMASK/GTMASK/GEMASK: Predicate masks       ||
|  |  SR_GLOBALTIMERLO/HI: Global timer                             ||
|  |  SR_SM_SPA_VERSION: SM architecture version                    ||
|  +----------------------------------------------------------------+|
+---------------------------------------------------------------------+
```

**Figure 4.1:** SM86 register file layout. All register types are shown with
their access scope and special properties. [CONFIRMED from oboromi source and
NVIDIA documentation.] [2][4]

### 4.3 Register Bank Architecture

The register file is organized into **banks** to enable parallel access by
execution units. Bank conflicts occur when multiple operands in a single
instruction map to the same bank, serializing accesses and reducing throughput.
[INFERRED from general Ampere architecture knowledge.] [2]

### 4.4 Occupancy Calculation

Occupancy is the ratio of active warps to the maximum warps per SM. For SM86,
the maximum concurrent warps is **48** (1,536 threads at 32 threads/warp).
[CONFIRMED] [2]

| Registers/Thread | Threads/SM | Warps/SM | Occupancy |
|---|---|---|---|
| 255 | 256 | 8 | 16.7% |
| 128 | 512 | 16 | 33.3% |
| 85 | 768 | 24 | 50.0% |
| 64 | 1,024 | 32 | 66.7% |
| 42 | 1,536 | 48 | 100.0% |
| 32 | 1,536 | 48 | 100.0% (capped by max warps) |

**Table 4.1:** SM86 occupancy vs registers per thread. Formula: `warps = min(48,
floor(65536 / (regs_per_thread × 32)))`. [CONFIRMED] [2]

### 4.5 Scoreboard

The SM86 scoreboard tracks **6 outstanding memory/dependency slots** per warp.
The DEPBAR instruction explicitly manages scoreboard dependencies, allowing the
compiler or programmer to express when results from long-latency operations
(e.g., global memory loads) are ready. [CONFIRMED from oboromi ISA spec:
`SCOREBOARD (SB0) = { SB(0..5) }`.] [4][5]

### 4.6 Barrier Registers

SM86 provides **64 barrier registers** (B0–B63) for inter-warp synchronization.
Barriers support SYNC, ARRIVE, RED, SCAN, and SYNCALL operations. [CONFIRMED
from oboromi ISA spec: `BarrierRegister B(0..63)=(0..63)`.] [4]

---

## 5. SASS ISA Reference

### 5.1 Overview

SASS (Shader Assembly) is the low-level instruction set executed by NVIDIA GPUs.
SM86 SASS uses **128-bit (16-byte) instruction words** encoded as two 64-bit
words. The oboromi decoder processes 128-bit instruction values via bitfield
extraction. [CONFIRMED from oboromi `sm86_decoder_generated.rs`: all decode
methods take `inst: u128`.] [4][5]

The SM86 ISA defines **1,271 instruction variants** across all functional
pipelines. [CONFIRMED from oboromi source comment: "1271 instructions".] [4]

### 5.2 Instruction Encoding Format

SM86 instructions are 128 bits wide. The encoding uses fixed-position bitfields
for opcode, operands, and control flags. [CONFIRMED from oboromi decoder.] [4]

```
+--128-bit SASS Instruction Word----------------------------------------+
|                                                                        |
|  Bits [11:0]   — Primary opcode (12 bits)                             |
|  Bits [14:12]  — Predicate guard (Pg, 3 bits)                         |
|  Bit  [15]     — Predicate negate (Pg_not)                            |
|  Bits [23:16]  — Destination register (Rd, 8 bits)                    |
|  Bits [31:24]  — Source register A (Ra, 8 bits)                       |
|  Bits [39:32]  — Source register B (Rb, 8 bits)                       |
|  Bits [53:40]  — Immediate / constant address / barname               |
|  Bits [63:54]  — Source register C (Rc) or immediate extension        |
|  Bits [71:64]  — Extended operand (Re, 8 bits)                        |
|  Bits [73:72]  — Size/format selector (E, Sz)                         |
|  Bits [75:74]  — Bitwise operation / negate / absolute               |
|  Bits [77:76]  — Destination format / saturation                      |
|  Bits [79:78]  — Stride / merge / rounding mode                       |
|  Bits [81:80]  — FTZ / atomic / cache control                         |
|  Bits [83:82]  — Pipeline / mode selector                             |
|  Bits [86:84]  — Compare operation / depth / mode                     |
|  Bits [90:87]  — Predicate write (Pp, 3 bits) + input register size   |
|  Bit  [91]     — Extended opcode bit (MSB of 13-bit primary opcode)   |
|  Bits [101:92] — Reserved / pipeline-specific fields                  |
|  Bits [103:102] — Performance metric predicate (pm_pred)              |
|  Bits [109:104] — Reserved                                            |
|  Bits [112:110] — Destination write scoreboard (dst_wr_sb)            |
|  Bits [115:113] — Source relative scoreboard (src_rel_sb)             |
|  Bits [121:116] — Request bit set (req_bit_set)                       |
|  Bits [127:122] — Extended opcode (opex, 6+5 bits)                    |
+------------------------------------------------------------------------+
```

**Figure 5.1:** SM86 SASS instruction encoding (128-bit). Bit positions are
derived from the oboromi decoder bitfield extraction patterns. Not all fields
are present in every instruction — the decoder uses mask+compare on the primary
opcode (bits [11:0] with bit 91 extension) to identify instruction types.
[CONFIRMED from oboromi source.] [4]

### 5.3 Operand Types

| Type | Syntax | Description |
|---|---|---|
| Register | R0–R253 | General-purpose 32-bit register [CONFIRMED] |
| Zero Register | RZ | Always reads as 0 (writes ignored) [CONFIRMED] |
| Uniform Register | UR0–UR62 | Warp-uniform 32-bit register [CONFIRMED] |
| Uniform Zero | URZ (UR63) | Always reads as 0 [CONFIRMED] |
| Predicate | P0–P6 | Predicate register (1-bit) [CONFIRMED] |
| Always True | PT | Predicate always true [CONFIRMED] |
| Immediate | imm16/imm32 | Inline constant [CONFIRMED] |
| Constant Bank | c[bank][offset] | Constant memory access [CONFIRMED] |
| Special Register | SR0–SR255 | Hardware state register [CONFIRMED] |
| Barrier Register | B0–B63 | Synchronization barrier [CONFIRMED] |
| Uniform Predicate | UP0–UP6 | Warp-uniform predicate [CONFIRMED] |
| Uniform Always True | UPT | Uniform predicate always true [CONFIRMED] |
| Register Pair | R0:R1 | 64-bit value in two registers [INFERRED] |

**Table 5.1:** SM86 operand types. [4][5]

### 5.4 Predication

All SM86 instructions support **predicated execution** via a 3-bit predicate
field (Pg) and a predicate-negate bit (Pg_not). When the specified predicate
is false (or true if negated), the instruction becomes a no-op — it does not
write its destination or produce side effects. [CONFIRMED from oboromi decoder:
`store_register_predicated()` method applies mask-select predication.] [4]

The special predicate register **PT (P7)** is always true. An instruction
guarded by `@PT` (or with Pg=7) always executes. [CONFIRMED from oboromi:
`if pred_idx == 7 { return; } // PT cannot be modified`.] [4]

### 5.5 Instruction Categories

#### 5.5.1 Arithmetic Instructions

| Mnemonic | Operation | Pipe | Latency | Description |
|---|---|---|---|---|
| FADD | Rd = Ra + Sc | fmalighter | 4–6 | FP32 addition [CONFIRMED] |
| FMUL | Rd = Ra × Sc | fmalighter | 4–6 | FP32 multiplication [CONFIRMED] |
| FFMA | Rd = Ra × Rb + Rc | fmalighter | 5–6 | FP32 fused multiply-add [CONFIRMED] |
| FADD32I | Rd = Ra + imm32 | fmalighter | 4–6 | FP32 add with 32-bit immediate [CONFIRMED] |
| FMUL32I | Rd = Ra × imm32 | fmalighter | 4–6 | FP32 mul with 32-bit immediate [CONFIRMED] |
| FFMA32I | Rd = Ra × Rb + imm32 | fmalighter | 5–6 | FP32 FMA with 32-bit immediate [CONFIRMED] |
| IADD3 | Rd = Ra + Rb + Rc | int | 6 | Three-input integer add [CONFIRMED] |
| IMAD | Rd = Ra × Rb + Rc | fmalighter | 5–6 | Integer multiply-add [CONFIRMED] |
| IMUL | Rd = Ra × Rb | fmalighter | 5–6 | Integer multiply [CONFIRMED] |
| IABS | Rd = |Ra| | int | 6 | Integer absolute value [CONFIRMED] |
| DADD | Rd = Ra + Sc | fma64lite | 10–12 | FP64 addition [CONFIRMED] |
| DMUL | Rd = Ra × Sc | fma64lite | 10–12 | FP64 multiplication [CONFIRMED] |
| DFMA | Rd = Ra × Rb + Rc | fma64lite | 10–12 | FP64 fused multiply-add [CONFIRMED] |
| HADD2 | Rd.H = Ra.H + Sc.H | fp16 | 5 | FP16 addition (packed) [CONFIRMED] |
| HFMA2 | Rd.H = Ra.H × Rb.H + Rc.H | fp16 | 5 | FP16 fused multiply-add (packed) [CONFIRMED] |
| HMUL2 | Rd.H = Ra.H × Sc.H | fp16 | 5 | FP16 multiplication (packed) [CONFIRMED] |
| MUFU | Rd = f(Ra) | mio | ~16–20 | Special functions: SIN, COS, RCP, RSQ, SQRT, EX2, LG2, TANH [CONFIRMED] |
| F2F | Rd = float(Ra) | mio | Variable | Float-to-float conversion [CONFIRMED] |
| F2I | Rd = int(Ra) | mio | Variable | Float-to-integer conversion [CONFIRMED] |
| I2F | Rd = float(Ra) | mio | Variable | Integer-to-float conversion [CONFIRMED] |
| F2FP | Pack FP16 from FP32 | mio | Variable | FP32 to packed FP16 [CONFIRMED] |
| F2IP | Pack int from float | mio | Variable | Float to packed integer [CONFIRMED] |
| FRND | Rd = round(Ra) | mio | Variable | Floating-point rounding [CONFIRMED] |
| FCHK | Float range check | mio | Variable | Check FP operand validity [CONFIRMED] |

**Table 5.2:** Arithmetic instruction summary. [4][5]

#### 5.5.2 Logic and Bitfield Instructions

| Mnemonic | Operation | Pipe | Latency | Description |
|---|---|---|---|---|
| LOP3 | Rd = LUT3(Ra, Rb, Rc) | int | 6 | 3-input logic via 8-bit LUT [CONFIRMED] |
| SHF | Rd = funnel_shift(Ra, Rb, imm) | int | 6 | Funnel shift (left or right) [CONFIRMED] |
| BFE | Rd = bitfield_extract(Ra, pos, len) | int | 6 | Bitfield extract [CONFIRMED] |
| BFI | Rd = bitfield_insert(Ra, Rb, pos, len) | int | 6 | Bitfield insert [CONFIRMED] |
| BREV | Rd = bit_reverse(Ra) | mio | Variable | Bit reversal [CONFIRMED] |
| BMSK | Rd = bitmask(Ra, width) | int | 6 | Generate bit mask [CONFIRMED] |
| SGXT | Rd = sign_extend(Ra, width) | int | 6 | Sign extend [CONFIRMED] |
| FLO | Rd = find_left_one(Ra) | mio | Variable | Find leftmost one bit [CONFIRMED] |
| CLZ | Rd = count_leading_zeros(Ra) | mio | Variable | Count leading zeros [CONFIRMED] |
| POPC | Rd = popcount(Ra) | mio | Variable | Population count [CONFIRMED] |
| PRMT | Rd = permute_bytes(Ra, Rb, Rc) | int | 6 | Byte permute [CONFIRMED] |
| LEA | Rd = Ra << imm + Rb | int | 6 | Load effective address [CONFIRMED] |
| SEL | Rd = pred ? Ra : Rb | int | 6 | Conditional select [CONFIRMED] |
| MOV | Rd = Ra | int | 6 | Register move [CONFIRMED] |
| PLOP3 | Pd = PLOP3(Pa, Pb, Pc) | int | 6 | Predicate 3-input logic [CONFIRMED] |

**Table 5.3:** Logic and bitfield instruction summary. [4][5]

#### 5.5.3 Control Flow Instructions

| Mnemonic | Operation | Pipe | Latency | Description |
|---|---|---|---|---|
| BRA | PC = target | cbu | 0 | Unconditional branch [CONFIRMED] |
| BRX | PC = Ra + offset | cbu | 0 | Indirect branch [CONFIRMED] |
| BRXU | PC = URa + offset | cbu | 0 | Uniform indirect branch [CONFIRMED] |
| CALL | push PC; PC = target | cbu | 0 | Subroutine call [CONFIRMED] |
| RET | PC = pop() | cbu | 0 | Return from subroutine [CONFIRMED] |
| EXIT | Terminate warp | cbu | 0 | Program exit [CONFIRMED] |
| BREAK | Break from loop | cbu | 0 | Loop break [CONFIRMED] |
| BSYNC | Sync at barrier label | cbu | 0 | Synchronization point [CONFIRMED] |
| BSSY | Register sync point | cbu | 0 | Register barrier sync [CONFIRMED] |
| BMOV | Move barrier state | cbu | 0 | Copy CBU state [CONFIRMED] |
| DEPBAR | Scoreboard dependency | fe | 0 | Explicit dependency barrier [CONFIRMED] |
| WARPSYNC | Warp synchronization | cbu | 0 | Wait for warp convergence [CONFIRMED] |
| YIELD | Yield warp | cbu | 0 | Voluntary warp yield [CONFIRMED] |
| NANOSLEEP | Sleep N cycles | cbu | N | Busy-wait for N cycles [CONFIRMED] |
| NOP | No operation | fe | 0 | No-op [CONFIRMED] |

**Table 5.4:** Control flow instruction summary. SM86 uses 0-delay branching —
branches do not introduce pipeline bubbles. [CONFIRMED from ISA spec.] [4][5]

#### 5.5.4 Memory Instructions

| Mnemonic | Operation | Pipe | Description |
|---|---|---|---|
| LDG | Rd = global[addr] | mio | Load from global memory [CONFIRMED] |
| STG | global[addr] = Ra | mio | Store to global memory [CONFIRMED] |
| LDL | Rd = local[addr] | mio | Load from local memory [CONFIRMED] |
| STL | local[addr] = Ra | mio | Store to local memory [CONFIRMED] |
| LDS | Rd = shared[addr] | mio | Load from shared memory [CONFIRMED] |
| STS | shared[addr] = Ra | mio | Store to shared memory [CONFIRMED] |
| LDGSTS | shared[addr] = global[addr] | mio | Async global-to-shared copy [CONFIRMED] |
| LDSM | Rd = shared_matrix[addr] | mio | Shared memory matrix load [CONFIRMED] |
| LDC | Rd = const[bank][offset] | mio | Load from constant memory [CONFIRMED] |
| LD | Rd = generic[addr] | mio | Generic address space load [CONFIRMED] |
| ST | generic[addr] = Ra | mio | Generic address space store [CONFIRMED] |
| ALD | Rd = attr[addr] | mio | Load attribute (vertex) [CONFIRMED] |
| AST | attr[addr] = Ra | mio | Store attribute [CONFIRMED] |
| ATOM | Rd = atomic_op(generic) | mio | Atomic operation on generic memory [CONFIRMED] |
| ATOMG | Rd = atomic_op(global) | mio | Atomic operation on global memory [CONFIRMED] |
| ATOMS | Rd = atomic_op(shared) | mio | Atomic operation on shared memory [CONFIRMED] |
| RED | atomic_op(generic) | mio | Atomic reduction (no return) [CONFIRMED] |
| CCTL | cache_control(addr) | mio | Cache control operations [CONFIRMED] |
| CCTLL | local_cache_control(addr) | mio | Local cache control [CONFIRMED] |
| CCTLT | texture_cache_control | mio | Texture cache control [CONFIRMED] |
| MEMBAR | memory_fence(scope) | mio | Memory barrier (CTA/GL/SYS/VC) [CONFIRMED] |
| LDGDEPBAR | Load dependency barrier | mio | Dependency after global loads [CONFIRMED] |
| ARRIVES | barrier_arrive(B, count) | mio | Arrive at barrier with count [CONFIRMED] |
| AL2P | Rd = Ra + offset | mio | Attribute to pixel address [CONFIRMED] |

**Table 5.5:** Memory instruction summary. [4][5]

Atomic operations support the following ALUs: ADD, MIN, MAX, INC, DEC, AND, OR,
XOR, EXCH, SAFEADD. Sizes: U32, S32, U64, S64, F32 (with FTZ rounding).
[CONFIRMED from oboromi ISA spec.] [4]

Cache control modes for LDG/STG:
- **CA** (cache all) — Cache in both L1 and L2 [CONFIRMED]
- **CG** (cache global) — Cache in L2 only [CONFIRMED]
- **CS** (cache streaming) — Streaming hint, evict-first [CONFIRMED]
- **CV** (cache volatile) — Volatile, bypass caches [CONFIRMED]
- **LU** (last use) — Hint that this is the last use [CONFIRMED]
- **CI** (cache invalidate) — Invalidate after read [CONFIRMED]

#### 5.5.5 Texture and Surface Instructions

| Mnemonic | Operation | Pipe | Description |
|---|---|---|---|
| TEX | Texture sample | mio | 1D/2D/3D texture fetch with filtering [CONFIRMED] |
| TLD | Texture load | mio | Texture load without filtering [CONFIRMED] |
| TLD4 | Texture load 4-tap | mio | Load 4 texels for gather [CONFIRMED] |
| TXQ | Texture query | mio | Query texture properties [CONFIRMED] |
| TMML | Texture mipmap level | mio | Query mipmap LOD [CONFIRMED] |
| TXD | Texture gradient | mio | Texture sample with explicit gradients [CONFIRMED] |
| TXA | Texture atomic | mio | Atomic on texture surface [CONFIRMED] |
| SULD | Surface load | mio | Load from surface (typed) [CONFIRMED] |
| SUST | Surface store | mio | Store to surface (typed) [CONFIRMED] |
| SUATOM | Surface atomic | mio | Atomic on surface [CONFIRMED] |
| SURED | Surface reduction | mio | Atomic reduction on surface [CONFIRMED] |
| SUQUERY | Surface query | mio | Query surface properties [CONFIRMED] |
| FOOTPRINT | Texture footprint | mio | Tile-based texture sampling [CONFIRMED] |

**Table 5.6:** Texture and surface instruction summary. [4][5]

#### 5.5.6 Tensor Core (MMA) Instructions

| Mnemonic | Operation | Pipe | Description |
|---|---|---|---|
| HMMA | Matrix multiply FP16 | fp16 | Half-precision MMA: D = A × B + C [CONFIRMED] |
| IMMA | Matrix multiply INT8 | int | Integer MMA: D = A × B + C [CONFIRMED] |
| DMMA | Matrix multiply FP64 | fma64lite | Double-precision MMA [CONFIRMED] |
| BMMA | Matrix multiply INT1 | int | Binary MMA: D = A × B + C [CONFIRMED] |
| CLMAD | Cross-lane multiply-add | fma64lite | Cross-lane multiply-add [CONFIRMED] |
| HFMA2.MMA | FP16 MMA variant | fma64lite | HFMA2 for MMA pipeline [CONFIRMED] |

**Table 5.7:** Tensor core MMA instruction summary. [4][5]

#### 5.5.7 Special and System Instructions

| Mnemonic | Operation | Pipe | Description |
|---|---|---|---|
| CS2R | Rd = SR[sr] | int | Read special register to GPR [CONFIRMED] |
| S2R | Rd = SR[sr] | mio | Read special register (mio pipe) [CONFIRMED] |
| B2R | Rd = barrier[B] | mio | Read barrier register [CONFIRMED] |
| R2B | barrier[B] = Ra | mio | Write barrier register [CONFIRMED] |
| VOTE | ballot/pred reduction | int | Warp-level vote (ANY/ALL/ballot) [CONFIRMED] |
| MATCH | Warp match | mio | Find matching threads in warp [CONFIRMED] |
| WARPSYNC | Warp barrier | cbu | Synchronize all threads in warp [CONFIRMED] |
| VOTEU | Uniform vote | udp | Uniform warp vote [CONFIRMED] |
| REDUX | Warp reduction | udp | Warp-level reduction (add/min/max) [CONFIRMED] |
| SHFL | Warp shuffle | mio | Register shuffle across warp lanes [CONFIRMED] |
| RPCMOV | PC register move | int | Move return PC register [CONFIRMED] |
| ERRBAR | Error barrier | mio | Error synchronization barrier [CONFIRMED] |
| PMTRIG | Performance trigger | fe | Performance counter trigger [CONFIRMED] |

**Table 5.8:** Special and system instruction summary. [4][5]

### 5.6 Pipeline Cross-Reference

The following table maps each instruction category to its execution pipeline
as defined in the SM86 ISA specification. [CONFIRMED from oboromi
`sm_86_instructions.txt`.] [4]

| Pipeline | Instructions | Sub-partitions | Notes |
|---|---|---|---|
| int_pipe | LOP3, SHF, IADD3, MOV, ISETP, FSETP, VOTE, SEL, P2R, R2P, LEA, PRMT, BMMA, IMMA, I2I | 4 | 1 per sub-partition/cycle |
| fmalighter_pipe | FFMA, FADD, FMUL, IMAD, IMUL, IDP, RRO | 4 | 1 per sub-partition/cycle |
| fp16_pipe | HADD2, HFMA2, HMUL2, HMMA, HSET2, HSETP2, HMNMX2 | 4 | 1 per sub-partition/cycle |
| fma64lite_pipe | DFMA, DADD, DMUL, DMMA, CLMAD, HFMA2.MMA, DSETP | 4 | 1 per 2 cycles |
| mio_pipe | LDG, STG, LDS, STS, ATOM, TEX, SULD, SUST, SHFL, BAR, S2R, B2R, MUFU, F2F, F2I, I2F, CCTL | 4 | Memory + texture + SFU |
| cbu_pipe | BRA, BRX, CALL, RET, EXIT, BREAK, BSYNC, BSSY, BMOV, WARPSYNC, YIELD, NANOSLEEP | 1 | Shared across SM |
| udp_pipe | UIADD3, ULOP3, UMOV, ULEA, USHF, ULDC, VOTEU, REDUX, S2UR, R2UR | 4 | Uniform data processing |
| ttu_pipe | TTUOPEN, TTUCLOSE, TTUGO, TTULD, TTUST, TTUMACROFUSE, TTUCCTL | 1 | Tensor core operations |
| fe_pipe | DEPBAR, NOP, PMTRIG, CSMTEST | 1 | Front-end / scoreboard |

**Table 5.9:** Pipeline cross-reference. [4][5]

---

## Citations

| # | Source | URL | Description | Accessed |
|---|---|---|---|---|
| [1] | Digital Foundry | https://www.digitalfoundry.net/articles/digitalfoundry-2025-nintendo-switch-2-the-digital-foundry-hardware-review | Nintendo Switch 2 hardware review with T239 specifications | 2025 |
| [2] | NVIDIA Ampere Tuning Guide | https://docs.nvidia.com/cuda/ampere-tuning-guide/index.html | Official SM86 architecture details, occupancy, register file | 2025 |
| [3] | TechPowerUp | https://www.techpowerup.com/336766/final-nintendo-switch-2-specifications-surface-cpu-gpu-memory-and-system-reservation | Final Switch 2 specs confirmation | 2025 |
| [4] | oboromi source | `core/src/gpu/sm86.rs`, `scripts/sm_86_instructions.txt` | SASS instruction definitions, register constants, decoder | Local |
| [5] | oboromi source | `scripts/sm_86_latencies.txt`, `core/src/gpu/sm86_decoder_generated.rs` | Instruction latency tables, 1271-instruction decoder | Local |
| [6] | NVIDIA Orin TRM | Technical Reference Manual for T234 (closest public T239 documentation) | SM architecture, memory hierarchy, RT/Tensor cores | 2024 |
| [7] | NVIDIA Ada Tuning Guide | https://docs.nvidia.com/cuda/ada-tuning-guide/index.html | Ada architecture comparison, Tensor Core capabilities | 2025 |

---

## Gap Analysis (Preview)

A full gap analysis comparing this reference against oboromi's existing GPU code
(`core/src/gpu/sm86.rs`, `core/src/gpu/spirv.rs`) will be provided in a
subsequent task. Key areas to examine:

1. **Decoder coverage** — oboromi's auto-generated decoder handles 1,271 instruction
   entries, but many implementations are `todo!()`. How many remain unimplemented?
2. **Register handling** — The current SPIR-V translator models R0–R253 + RZ (254
   registers) but the full SM86 register file includes uniform registers (UR0–UR62),
   uniform predicates (UP0–UP6), and special registers that need separate handling.
3. **Predication model** — The current branchless predication implementation (mask-select
   pattern) correctly handles P0–P6 + PT but does not yet model uniform predicates.
4. **Pipeline scheduling** — The decoder is purely sequential; no pipeline scheduling
   or scoreboard management is implemented. DEPBAR is decoded but not enforced.
5. **Memory hierarchy** — Cache control (CCTL, CCTLL, CCTLT) and memory barriers
   (MEMBAR) are decoded but not modeled in the SPIR-V translation.

---

*Document generated as part of oboromi M001/S01. Next sections will cover:
RT Cores, Tensor Cores, Ada hybrid features, DLSS support, NVN2 API overview,
and complete gap analysis.*
