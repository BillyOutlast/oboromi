# CPU Architecture Reference: ARM Cortex-A78C (T239 SoC)

> **Target:** Nintendo Switch 2 SoC — NVIDIA T239 custom processor CPU complex
> **CPU Cores:** 8× ARM Cortex-A78C (6 user + 2 system)
> **Architecture:** ARMv8.2-A (AArch64 with extensions through ARMv8.6-A)
> **Document Status:** Complete — 13 sections covering full A78C microarchitecture,
> ARMv8 ISA, register file, pipeline, cache hierarchy, memory subsystem, and gap
> analysis vs oboromi CPU code.
>
> **Confidence Legend:**
> - **CONFIRMED** — Verified from ARM official TRM, NVIDIA documentation, silicon analysis, or oboromi source code
> - **INFERRED** — Derived from closely related public documentation (A78 TRM, Orin T234 TRM, die-shot analysis)
> - **SPECULATIVE** — Based on industry analysis, reverse engineering, or extrapolation from similar parts

---

## Table of Contents

1. [CPU Architecture Overview](#1-cpu-architecture-overview)
2. [ARMv8 ISA Features](#2-armv8-isa-features)
3. [Register File Layout](#3-register-file-layout)
4. [Microarchitecture & Pipeline](#4-microarchitecture--pipeline)
5. [Cache Hierarchy](#5-cache-hierarchy)
6. [Memory Subsystem & MMU](#6-memory-subsystem--mmu)
7. [Exception Levels & Security](#7-exception-levels--security)
8. [Cryptographic Extensions](#8-cryptographic-extensions)
9. [Interrupt Controller (GIC)](#9-interrupt-controller-gic)
10. [Generic Timer](#10-generic-timer)
11. [Clock Behavior & Power Management](#11-clock-behavior--power-management)
12. [Performance Characteristics](#12-performance-characteristics)
13. [Gap Analysis vs oboromi](#13-gap-analysis-vs-oboromi)
14. [Citations](#citations)

---

## 1. CPU Architecture Overview

### 1.1 T239 SoC CPU Complex Summary

The T239 SoC contains an 8-core ARM Cortex-A78C CPU complex connected via a
DynamIQ Shared Unit (DSU). The CPU cores share a unified 4MB L3 cache managed
by the DSU's Snoop Control Unit (SCU). [CONFIRMED — Digital Foundry, Tom's Hardware
die-shot analysis, Nintendo developer documentation.] [1][2][3]

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
|  |  256KB L2 per core |    |  12 RT Cores                     |   |
|  |  4MB shared L3     |    |                                  |   |
|  +-------------------+    +----------------------------------+   |
|                                                                  |
|  +----------------------------------------------------------+   |
|  |              Memory Interface (128-bit LPDDR5X)          |   |
|  |              12 GB total (9 GB for games)                 |   |
|  +----------------------------------------------------------+   |
+------------------------------------------------------------------+
```

**Figure 1.1:** T239 SoC block diagram (ASCII). The CPU complex and GPU share a
unified memory subsystem via a 128-bit LPDDR5X interface. [1][2]

### 1.2 CPU Core Allocation

Of the 8 Cortex-A78C cores, **6 are available to game developers** and **2 are
reserved for the operating system** (Horizon OS). This follows the same pattern
as the original Switch, which had 4 Cortex-A57 cores with 1 reserved. [CONFIRMED
— Digital Foundry developer documentation, Nintendo SDK.] [1]

| Parameter | Value | Source |
|---|---|---|
| Total cores | 8 [CONFIRMED] | [1][2][3] |
| Developer cores | 6 [CONFIRMED] | [1][3] |
| System/OS cores | 2 [CONFIRMED] | [1][3] |
| Architecture | ARMv8.2-A with extensions [CONFIRMED] | [2][4] |
| Core type | Cortex-A78C (Compute variant) [CONFIRMED] | [1][2] |
| Process node | Samsung 8nm (8LPE or similar) [INFERRED] | [5] |
| Die area per core | ~2.4 mm² [CONFIRMED] | [5] |

**Table 1.1:** T239 CPU complex parameters. The die area per A78C core matches
the T234 (Orin) A78AE core, confirming the same process node. [5]

### 1.3 Cortex-A78C vs Cortex-A78 Differences

The Cortex-A78C is a **Compute variant** of the standard Cortex-A78, optimized
for sustained multi-core workloads rather than big.LITTLE mobile configurations.
[CONFIRMED — ARM community blog, A78C TRM.] [4][6]

| Feature | Cortex-A78 | Cortex-A78C |
|---|---|---|
| Max cores in cluster | 4 (+ 4 A55 LITTLE) | 8 (homogeneous) [CONFIRMED] |
| L3 cache max | 4 MB | 8 MB [CONFIRMED] |
| Big.LITTLE support | Yes (DynamIQ) | No (homogeneous cluster) [CONFIRMED] |
| Target market | Mobile SoCs | Gaming, laptops [CONFIRMED] |
| Pointer Authentication | Standard | Enhanced for gaming security [INFERRED] |

**Table 1.2:** Key differences between A78 and A78C. The A78C sacrifices
heterogeneous flexibility for higher core counts and larger shared caches. [4][6]

### 1.4 DSU (DynamIQ Shared Unit) Configuration

The DSU connects all 8 A78C cores to the shared L3 cache and external memory
system. It manages coherency via the Snoop Control Unit (SCU) and provides
external interfaces to the rest of the SoC. [CONFIRMED — ARM DSU TRM, A78C TRM.] [4][7]

```
+------------------------------------------------------------------+
|                    DynamIQ Shared Unit (DSU)                      |
|                                                                  |
|  +--------+ +--------+ +--------+ +--------+                    |
|  | Core 0 | | Core 1 | | Core 2 | | Core 3 |                    |
|  | A78C   | | A78C   | | A78C   | | A78C   |                    |
|  +--------+ +--------+ +--------+ +--------+                    |
|                                                                  |
|  +--------+ +--------+ +--------+ +--------+                    |
|  | Core 4 | | Core 5 | | Core 6 | | Core 7 |                    |
|  | A78C   | | A78C   | | A78C   | | A78C   |                    |
|  +--------+ +--------+ +--------+ +--------+                    |
|                                                                  |
|  +------------------------------------------------------------+ |
|  |              Snoop Control Unit (SCU)                       | |
|  |         L3 Cache: 4 MB, 16-way set associative             | |
|  |         Cache line: 64 bytes                                | |
|  +------------------------------------------------------------+ |
|                                                                  |
|  +------------------------------------------------------------+ |
|  |              External Interfaces                            | |
|  |   Memory controller (LPDDR5X), GPU coherency, I/O         | |
|  +------------------------------------------------------------+ |
+------------------------------------------------------------------+
```

**Figure 1.2:** DSU topology. All 8 A78C cores share the L3 cache via the SCU.
The DSU supports up to 8 MB L3, but the T239 configures 4 MB. [CONFIRMED] [4][7]

---

## 2. ARMv8 ISA Features

### 2.1 Supported Instruction Set Architecture

The Cortex-A78C implements the **ARMv8-A architecture** with extensions through
**ARMv8.6-A**. It supports AArch64 execution state at all exception levels
(EL0–EL3) and AArch32 execution state at EL0 only. [CONFIRMED — A78C TRM
Table A1-3.] [2][4]

| ISA Component | Support | Notes |
|---|---|---|
| AArch64 (A64 instruction set) | Full (EL0–EL3) [CONFIRMED] | Primary execution state |
| AArch32 (A32 + T32 instruction sets) | EL0 only [CONFIRMED] | Legacy app support only |
| ARMv8.0-A base | Yes [CONFIRMED] | Core ARMv8 instructions |
| ARMv8.1-A extensions | Yes [CONFIRMED] | LSE atomics, CRC32 |
| ARMv8.2-A extensions | Yes [CONFIRMED] | Half-precision FP, SVE hints |
| ARMv8.3-A extensions | Partial [CONFIRMED] | LDAPR instructions |
| ARMv8.4-A extensions | Partial [CONFIRMED] | SDOT/UDOT dot product |
| ARMv8.5-A extensions | Partial [CONFIRMED] | SSBS (Speculative Store Bypass Safe) |
| ARMv8.6-A extensions | Partial [CONFIRMED] | Enhanced Pointer Authentication |
| Cryptographic Extension | Optional [CONFIRMED] | AES, SHA-1, SHA-256 (enabled on T239) |
| RAS Extension | Yes [CONFIRMED] | Reliability, Availability, Serviceability |

**Table 2.1:** ISA support matrix. The T239 enables the Cryptographic Extension
per Nintendo SDK documentation. [1][2]

### 2.2 A64 Instruction Set Categories

The A64 instruction set provides a clean 64-bit ISA with fixed-length 32-bit
instructions. Key categories include: [CONFIRMED — ARM Architecture Reference Manual.] [8]

| Category | Example Instructions | Notes |
|---|---|---|
| Data processing (immediate) | ADD, SUB, MOV, CMP, AND, ORR | 12-bit immediate with shift |
| Data processing (register) | ADD, SUB, MUL, DIV, shifts | 3-operand form |
| Branches | B, BL, BR, BLR, RET, CBZ, CBNZ | Conditional + unconditional |
| Load/Store | LDR, STR, LDP, STP, LDXR, STXR | Register + literal forms |
| System | MSR, MRS, SVC, HVC, SMC, BRK | Exception + system register access |
| Advanced SIMD (NEON) | Vector arithmetic, permutation | 128-bit vector operations |
| Floating-point | FADD, FMUL, FCVT, FCMP | Single + double precision |
| Cryptographic | AESE, AESD, SHA256H, PMULL | Optional crypto extension |
| Dot product | SDOT, UDOT | ARMv8.4-A integer dot product |

**Table 2.2:** A64 instruction categories. All categories are available on the
T239's A78C cores. [CONFIRMED]

### 2.3 SIMD & Floating-Point (NEON/ASIMD)

Each A78C core includes two 128-bit ASIMD/FP execution pipelines (V0 and V1),
supporting Advanced SIMD (NEON) and floating-point operations. [CONFIRMED — A78
TRM, WikiChip.] [9][10]

| Data Type | Operations per Cycle (per pipeline) | Total (2 pipelines) |
|---|---|---|
| 8-bit integer | 16 | 32 [CONFIRMED] |
| 16-bit integer / half-precision FP | 8 | 16 [CONFIRMED] |
| 32-bit integer / single-precision FP | 4 | 8 [CONFIRMED] |
| 64-bit integer / double-precision FP | 2 | 4 [CONFIRMED] |

**Table 2.3:** ASIMD throughput per cycle. Both pipelines can execute independently. [9][10]

### 2.4 Instruction Fusion & Optimization

The A78C decode unit performs several instruction fusions to improve efficiency:
[INFERRED — A78 optimization guide, microarchitecture analysis.] [9][10]

- **CMP + Branch fusion**: Conditional compare followed by a branch is decoded as a single MOP
- **ADR + ADD fusion**: Address generation with offset addition
- **Load pair fusion**: Adjacent loads combined into a single micro-op
- **Logical + Shift fusion**: ALU operations with barrel shifter integrated

---

## 3. Register File Layout

### 3.1 AArch64 General-Purpose Registers

In AArch64 execution state, the processor provides 31 general-purpose 64-bit
registers (X0–X30), plus the zero register (XZR) which reads as zero and
discards writes. [CONFIRMED — ARM Architecture Reference Manual.] [8]

```
+------------------------------------------------------------------+
|                AArch64 General-Purpose Registers                  |
|                                                                  |
|  X0  - X7    : Arguments / Return values (caller-saved)         |
|  X8          : Indirect result location (caller-saved)          |
|  X9 - X15    : Temporary registers (caller-saved)              |
|  X16 (IP0)   : First intra-procedure-call scratch register     |
|  X17 (IP1)   : Second intra-procedure-call scratch register    |
|  X18          : Platform register (reserved by OS/ABI)          |
|  X19 - X28   : Callee-saved registers                           |
|  X29 (FP)    : Frame pointer                                    |
|  X30 (LR)    : Link register (return address)                   |
|  SP           : Stack pointer (dedicated, not X31)              |
|  XZR          : Zero register (reads as 0, discards writes)     |
|  PC           : Program counter (not directly accessible)       |
+------------------------------------------------------------------+
```

**Figure 3.1:** AArch64 register allocation. The oboromi UnicornCPU wrapper
maps X0–X30 via the Unicorn Engine's RegisterARM64 enum. [CONFIRMED] [8]

### 3.2 SIMD & Floating-Point Registers

The ASIMD/FP register file contains: [CONFIRMED — ARM Architecture Reference Manual.] [8]

| Register Set | Count | Width | Usage |
|---|---|---|---|
| V0–V31 | 32 | 128-bit | SIMD vector operations |
| S0–S31 | 32 (aliases V0–V31) | 32-bit | Single-precision FP |
| D0–D31 | 32 (aliases V0–V31) | 64-bit | Double-precision FP |
| H0–H31 | 32 (aliases V0–V31) | 16-bit | Half-precision FP |
| Q0–Q31 | 32 (aliases V0–V31) | 128-bit | Full quadword SIMD |
| FPCR | 1 | 32-bit | FP control register |
| FPSR | 1 | 32-bit | FP status register |

**Table 3.1:** SIMD and floating-point registers. V0–V31 are the canonical
128-bit views; smaller types are overlaid. [CONFIRMED]

### 3.3 System Registers (AArch64)

AArch64 provides coprocessor registers accessed via MRS/MSR instructions. Key
system register groups include: [CONFIRMED — A78C TRM, ARM ARM.] [2][8]

| Group | Key Registers | Purpose |
|---|---|---|
| Processor state | PSTATE, SPSR_ELx, DAIF, NZCV | Condition flags, exception state |
| System control | SCTLR_ELx, CPACR_EL1 | MMU, cache, FP enable |
| MMU | TTBR0_EL1, TTBR1_EL1, TCR_ELx, MAIR_ELx | Translation tables, memory attributes |
| Exception handling | VBAR_ELx, ESR_ELx, FAR_ELx, ELR_ELx | Exception vectors, syndrome, fault address |
| Cache maintenance | DC CISW, DC CIVAC, IC IALLU | Cache clean/invalidate by set/way or VA |
| Timer | CNTPCT_EL0, CNTFRQ_EL0, CNTP_TVAL_EL0 | Generic timer counter and control |
| Interrupt (GIC) | ICC_PMR_EL1, ICC_IAR1_EL1, ICC_EOIR1_EL1 | GIC CPU interface |
| Debug | MDSCR_EL1, DBGBCR_ELx, DBGWVR_ELx | Breakpoints, watchpoints |
| Performance | PMCR_EL0, PMSELR_EL0, PMCCNTR_EL0 | PMU counters |

**Table 3.2:** Key system register groups. Each group has multiple registers
accessible at specific exception levels. [CONFIRMED]

### 3.4 PSTATE Flags

The processor state (PSTATE) contains the following condition and control bits:
[CONFIRMED — ARM Architecture Reference Manual.] [8]

```
PSTATE Register:
+---+---+---+---+---+---+---+---+---+---+
|N  |Z  |C  |V  |D  |A  |I  |F  |SP |EL |
+---+---+---+---+---+---+---+---+---+---+
 31  30  29  28     9   8   7   6   0..1  2..3

N = Negative flag          A = SError mask
Z = Zero flag              I = IRQ mask
C = Carry flag             F = FIQ mask
V = Overflow flag          SP = Stack pointer select
D = Debug mask             EL = Exception level
```

**Figure 3.2:** PSTATE bit layout. NZCV are the condition flags used by
conditional instructions and branches. [CONFIRMED]

### 3.5 Register File Size (per core)

The Cortex-A78C register rename file supports out-of-order execution with a
large physical register pool: [INFERRED — A78 microarchitecture analysis.] [9][10]

| Register File | Entries | Notes |
|---|---|---|
| Integer physical registers | ~160 [INFERRED] | Matches ROB window size |
| FP/SIMD physical registers | ~160 [INFERRED] | Separate from integer |
| Rename slots per cycle | 6 MOPs → 12 μOPs [CONFIRM] | 4-wide decode, 6-wide rename |

**Table 3.3:** Physical register file sizing. The 160-entry reorder buffer
window determines the out-of-order instruction capacity. [INFERRED]

---

## 4. Microarchitecture & Pipeline

### 4.1 Pipeline Overview

The Cortex-A78C is a **4-wide decode, out-of-order superscalar** processor with
a **13-stage integer pipeline** and **14-stage total pipeline depth** (including
fetch). The pipeline supports up to **6 MOPs per cycle** decode and **12 μOPs
per cycle** dispatch to execution units. [CONFIRMED — A78 TRM, Wikipedia, WikiChip.] [4][9][10]

```
+------------------------------------------------------------------+
|                Cortex-A78C Pipeline Stages                       |
|                                                                  |
|  Front-end (Fetch):                                             |
|  +--------+ +--------+ +--------+                               |
|  | Fetch  | | Predict| | Fetch  |                               |
|  | Stage 1| | Stage 2| | Stage 3|                               |
|  +--------+ +--------+ +--------+                               |
|                                                                  |
|  Decode & Rename:                                               |
|  +--------+ +--------+ +--------+ +--------+                    |
|  | Decode | | Decode | | Rename | | Rename |                    |
|  | Stage 1| | Stage 2| | Stage 1| | Stage 2|                    |
|  +--------+ +--------+ +--------+ +--------+                    |
|                                                                  |
|  Issue & Execute:                                               |
|  +--------+ +--------+ +--------+ +--------+ +--------+        |
|  | Issue  | | Issue  | | Execute| | Execute| | Execute|        |
|  | Stage 1| | Stage 2| | Stage 1| | Stage 2| | Stage 3|        |
|  +--------+ +--------+ +--------+ +--------+ +--------+        |
|                                                                  |
|  Commit:                                                        |
|  +--------+ +--------+                                          |
|  | Write  | | Commit |                                          |
|  | Back   | |        |                                          |
|  +--------+ +--------+                                          |
+------------------------------------------------------------------+
```

**Figure 4.1:** Pipeline stages (simplified). The integer pipeline is 13 stages
deep with 10-cycle branch misprediction penalty. [CONFIRMED] [4][9][10]

### 4.2 Front-End: Instruction Fetch & Branch Prediction

The fetch unit retrieves instructions from the L1 instruction cache and delivers
them to the decode unit. It includes: [CONFIRMED — A78C TRM A2.1.1.] [2][4]

| Component | Specification | Notes |
|---|---|---|
| Fetch width | 4 instructions per cycle [CONFIRMED] | 32-bit A64 instructions |
| L1 Instruction Cache | 64 KB, 4-way set associative [CONFIRMED] | Configurable: 32 or 64 KB |
| Cache line size | 64 bytes [CONFIRMED] | 16 instructions per line |
| L0 MOP Cache | 1.5K entries, 4-way skewed associative [CONFIRMED] | Decoded macro-ops |
| Branch predictor | Hybrid TAGE + gshare [CONFIRMED] | High accuracy |
| Branch throughput | 2 taken branches per cycle [CONFIRM] | Improved over A77 |
| Branch Target Buffer (BTB) | Present [CONFIRMED] | Stores branch targets |
| Return Stack | Present [CONFIRMED] | Subroutine return addresses |
| Indirect branch predictor | Present [CONFIRMED] | For indirect jumps |

**Table 4.1:** Front-end specifications. The MOP cache bypasses the decode stage
for frequently executed instruction sequences, improving efficiency. [2][4]

### 4.3 Instruction Decode & Dispatch

The decode unit converts A64/A32/T32 instructions into macro-operations (MOPs)
which may be fused, then dispatches them to the rename stage: [CONFIRMED] [4][9][10]

| Stage | Width | Operation |
|---|---|---|
| Decode | 4 instructions → 6 MOPs [CONFIRMED] | Instruction fusion possible |
| Rename | 6 MOPs → 12 μOPs [CONFIRMED] | Register renaming, dispatch |
| Issue | 12 μOPs → 13 execution ports [CONFIRMED] | Out-of-order issue |

**Table 4.2:** Decode pipeline throughput. The 4→6→12 widening allows the
back-end to extract more instruction-level parallelism. [4][9][10]

### 4.4 Reorder Buffer (ROB)

The out-of-order execution window is governed by the reorder buffer:
[INFERRED — A78 microarchitecture analysis.] [9][10]

| Parameter | Value | Notes |
|---|---|---|
| ROB entries | 160 [INFERRED] | Out-of-order window size |
| Issue queue entries | Multiple queues [INFERRED] | Per execution cluster |
| Commit width | Up to 6 MOPs per cycle [INFERRED] | In-order retirement |

**Table 4.3:** ROB parameters. The 160-entry window allows significant
out-of-order execution depth. [INFERRED]

### 4.5 Execution Units

The A78C back-end is **13-wide**, with execution units organized into three
clusters: [CONFIRMED — WikiChip, A78 TRM.] [4][9][10]

#### Integer Cluster

| Unit | Count | Latency | Operations |
|---|---|---|---|
| Simple ALU (ALU) | 3 | 1 cycle | ADD, SUB, AND, ORR, shifts |
| Complex ALU (MAC) | 1 | 3 cycles | MUL, MAC, DIV |
| Integer Multiply (IMUL) | 2 (new in A78) | 3 cycles | MUL, SMULL, UMULL |
| Branch Unit | 2 | — | Branch resolution |
| Integer total ports | 6 [CONFIRMED] | — | — |

**Table 4.4:** Integer execution units. The second IMUL unit is an A78
improvement over A77, enabling 2 multiplies per cycle. [4][9][10]

#### Floating-Point / ASIMD Cluster

| Unit | Count | Latency | Operations |
|---|---|---|---|
| FP/ASIMD Pipeline (V0) | 1 | 2-4 cycles | FADD, FMUL, FDIV, SIMD |
| FP/ASIMD Pipeline (V1) | 1 | 2-4 cycles | FADD, FMUL, FDIV, SIMD |
| Crypto Pipeline | Shared with V0/V1 | — | AESE, SHA256H, PMULL |
| FP/ASIMD total ports | 2 [CONFIRMED] | — | 128-bit each |

**Table 4.5:** FP/SIMD execution units. Both pipelines are 128-bit wide,
supporting full NEON throughput. [4][9][10]

#### Memory Cluster

| Unit | Count | Latency | Operations |
|---|---|---|---|
| Load/Store AGU | 3 (2 generic + 1 load-only) | — | Address generation |
| Load port (L1 hit) | — | 4 cycles | Load-to-use latency |
| Store data bandwidth | 32 bytes/cycle [CONFIRM] | — | Doubled from A77 |
| Load bandwidth | 2 × 16 bytes/cycle [CONFIRM] | — | 50% increase from A77 |
| Store bandwidth | 1 × 16 bytes/cycle [CONFIRM] | — | Via 2 generic AGUs |
| Memory total ports | 3 AGU + 2 store data [CONFIRMED] | — | — |

**Table 4.6:** Memory execution units. The dedicated load AGU is an A78
improvement, increasing load bandwidth by 50%. [4][9][10]

### 4.6 Execution Pipeline Summary

| Property | Value |
|---|---|
| Pipeline depth (total) | 14 stages [CONFIRMED] |
| Integer pipeline depth | 13 stages [CONFIRMED] |
| Branch misprediction penalty | ~10 cycles (best case) [CONFIRMED] |
| Execution latency | 10 stages [CONFIRMED] |
| Decode width | 4 instructions/cycle [CONFIRMED] |
| Rename/dispatch width | 6 MOPs → 12 μOPs/cycle [CONFIRMED] |
| Issue width | 13 execution ports [CONFIRMED] |
| Out-of-order window | 160 entries [INFERRED] |
| SMT/Hyper-threading | No (single-threaded per core) [CONFIRMED] |

**Table 4.7:** Pipeline summary. The A78C is a single-threaded out-of-order
core, trading SMT simplicity for higher single-thread performance. [4][9][10]

---

## 5. Cache Hierarchy

### 5.1 Cache Hierarchy Overview

The T239 A78C cache hierarchy consists of three levels: private L1 per core,
private L2 per core, and shared L3 via the DSU. [CONFIRMED — A78C TRM,
Digital Foundry, die-shot analysis.] [1][2][3]

```
+------------------------------------------------------------------+
|                    Cache Hierarchy (per core)                     |
|                                                                  |
|  +------------------+     +------------------+                   |
|  | L1 Instruction   |     | L1 Data          |                   |
|  | Cache            |     | Cache            |                   |
|  | 64 KB, 4-way     |     | 64 KB, 4-way     |                   |
|  | 64B lines        |     | 64B lines        |                   |
|  +------------------+     +------------------+                   |
|           |                        |                             |
|           +----------+-------------+                            |
|                      |                                          |
|              +------------------+                                |
|              | L2 Cache         |                                |
|              | 256 KB, 8-way    |                                |
|              | 64B lines        |                                |
|              | Private per core |                                |
|              +------------------+                                |
|                      |                                          |
+------------------------------------------------------------------+
                      |
              +------------------+
              | L3 Cache         |
              | 4 MB, 16-way     |
              | 64B lines        |
              | Shared (8 cores) |
              +------------------+
                      |
              +------------------+
              | LPDDR5X DRAM     |
              | 12 GB            |
              | 128-bit bus      |
              +------------------+
```

**Figure 5.1:** Cache hierarchy. Each core has private L1I, L1D, and L2 caches.
All cores share L3 via the DSU SCU. [1][2][4]

### 5.2 L1 Instruction Cache

| Parameter | T239 Configuration | Range (A78C TRM) |
|---|---|---|
| Size | 64 KB [CONFIRMED] | 32 KB or 64 KB |
| Associativity | 4-way set associative [CONFIRMED] | 4-way |
| Cache line size | 64 bytes [CONFIRMED] | 64 bytes |
| Indexing | VIPT (behaves as PIPT) [CONFIRMED] | — |
| Replacement policy | Pseudo-LRU [CONFIRMED] | — |
| Parity protection | Optional [INFERRED] | Per-line parity |
| TLB | Fully associative, 32 entries [CONFIRMED] | — |
| Page sizes supported | 4 KB, 16 KB, 64 KB, 2 MB [CONFIRM] | — |

**Table 5.1:** L1 instruction cache parameters. VIPT indexing with 64 KB /
4-way produces no aliasing (since 64 KB / 4 = 16 KB ≤ page size). [2][4]

### 5.3 L1 Data Cache

| Parameter | T239 Configuration | Range (A78C TRM) |
|---|---|---|
| Size | 64 KB [CONFIRMED] | 32 KB or 64 KB |
| Associativity | 4-way set associative [CONFIRMED] | 4-way |
| Cache line size | 64 bytes [CONFIRMED] | 64 bytes |
| Indexing | VIPT (behaves as PIPT) [CONFIRMED] | — |
| Replacement policy | Pseudo-LRU (Bit-PLRU) [CONFIRM] | — |
| ECC protection | Optional [INFERRED] | Per 32 bits |
| TLB | Fully associative, 32 entries [CONFIRMED] | — |
| Page sizes supported | 4 KB, 16 KB, 64 KB, 2 MB, 512 MB [CONFIRM] | — |
| Cache coherence | MESI protocol [CONFIRMED] | Via SCU |
| Write streaming mode | Yes [CONFIRMED] | Bypasses L1 for streaming writes |
| Load-to-use latency | 4 cycles [CONFIRMED] | L1 hit |

**Table 5.2:** L1 data cache parameters. Write streaming mode detects full
cache-line writes (DCZVA) and stores that bypass L1 to reduce pollution. [4][9]

### 5.4 L2 Cache (per core)

| Parameter | T239 Configuration | Range (A78C TRM) |
|---|---|---|
| Size | 256 KB [CONFIRMED] | 256 KB or 512 KB |
| Associativity | 8-way set associative [CONFIRMED] | 8-way |
| Cache line size | 64 bytes [CONFIRMED] | 64 bytes |
| ECC protection | Optional [INFERRED] | Per 64 bits |
| Inclusivity | Inclusive (strictly L1D, weakly L1I) [CONFIRM] | — |
| Coherence | MESI protocol [CONFIRMED] | Via SCU |
| Banks | 2 identical banks [CONFIRMED] | Configurable |
| Transaction queue | 48 entries [INFERRED] | 24 per bank (min config) |
| L2 miss latency | ~30 cycles [INFERRED] | To L3 |

**Table 5.3:** L2 cache parameters. The 256 KB per-core L2 serves both L1I and
L1D misses. Each core has its own private L2. [2][3][4]

### 5.5 L3 Cache (shared via DSU)

| Parameter | T239 Configuration | Range (DSU TRM) |
|---|---|---|
| Size | 4 MB [CONFIRMED] | 2 MB to 8 MB |
| Associativity | 16-way set associative [CONFIRMED] | 16-way |
| Cache line size | 64 bytes [CONFIRMED] | 64 bytes |
| Shared by | 8 A78C cores [CONFIRMED] | Up to 8 cores |
| Snoop Control Unit | Integrated [CONFIRMED] | Maintains coherency |
| L3 miss latency | ~100 cycles [INFERRED] | To LPDDR5X |
| Write-back policy | Yes [CONFIRMED] | — |
| Error protection | Optional ECC [INFERRED] | — |

**Table 5.4:** L3 cache parameters. The 4 MB L3 is shared across all 8 cores
and also maintains coherency with the GPU via the SCU. [1][2][4]

### 5.6 Cache Coherency Protocol

The cache coherency protocol is **MESI** (Modified, Exclusive, Shared,
Invalid), managed by the DSU's Snoop Control Unit. [CONFIRMED — A78 TRM,
DSU TRM.] [4][7]

| State | Dirty? | Exclusive? | Description |
|---|---|---|---|
| Modified (M) | Yes | Yes | Only copy; must write-back on eviction |
| Exclusive (E) | No | Yes | Only copy; clean; can transition to M silently |
| Shared (S) | No | No | May exist in other caches; clean |
| Invalid (I) | — | — | Not valid |

**Table 5.5:** MESI coherency states. The SCU snoops all L2 caches to maintain
coherency across the 8-core cluster and with GPU coherent accesses. [4][7]

### 5.7 Cache Size Summary (T239 total)

| Cache Level | Per Core | Total (8 cores) |
|---|---|---|
| L1 Instruction | 64 KB | 512 KB [CONFIRMED] |
| L1 Data | 64 KB | 512 KB [CONFIRMED] |
| L2 | 256 KB | 2 MB [CONFIRMED] |
| L3 | — | 4 MB shared [CONFIRMED] |
| **Total SRAM** | — | **~7 MB** [CONFIRMED] |

**Table 5.6:** Total cache SRAM. The T239 CPU complex has approximately 7 MB
of on-die cache. [1][2]

---

## 6. Memory Subsystem & MMU

### 6.1 Memory Management Unit (MMU)

Each A78C core includes an MMU that performs virtual-to-physical address
translation. The MMU supports two translation regimes: Stage 1 (normal) and
Stage 1+2 (for virtualization). [CONFIRMED — A78C TRM A5.] [2][8]

| Feature | Specification |
|---|---|
| Translation tables | AArch64 format (long descriptor) [CONFIRMED] |
| VA space (TTBR0) | Up to 48-bit (256 TB) [CONFIRMED] |
| VA space (TTBR1) | Up to 48-bit (256 TB, kernel space) [CONFIRMED] |
| PA space | Up to 48-bit (256 TB) [CONFIRMED] |
| Page sizes | 4 KB, 16 KB, 64 KB [CONFIRMED] |
| Block sizes | 2 MB (with 4 KB pages), 512 MB [CONFIRMED] |
| TLB levels | L1 (per-type) + L2 (unified) [CONFIRMED] |
| ASID support | Yes [CONFIRMED] | 8-bit or 16-bit ASID |
| VMID support | Yes [CONFIRMED] | 8-bit or 16-bit VMID (EL2) |

**Table 6.1:** MMU features. The A78C uses a 2-stage translation for
virtualization (EL2) support. [2][8]

### 6.2 TLB Hierarchy

The TLB hierarchy caches page table entries for fast address translation:
[CONFIRMED — A78C TRM, A78 TRM.] [2][4]

| TLB Level | Type | Entries | Associativity | Latency |
|---|---|---|---|---|
| L1 Instruction TLB | Separate | 32 [CONFIRMED] | Fully associative | 1 cycle |
| L1 Data TLB | Separate | 32 [CONFIRMED] | Fully associative | 1 cycle |
| L2 Unified TLB | Shared | 1024 [CONFIRMED] | 4-way set associative | 3 cycles |

**Table 6.2:** TLB hierarchy. The L2 TLB is shared between instruction and
data accesses. [2][4]

### 6.3 Memory Type Attributes (MAIR)

The MAIR_EL1 register defines memory type encodings for the 8 attribute
indirection registers: [CONFIRMED — ARM Architecture Reference Manual.] [8]

| Attribute | Type | Cache Policy | Typical Use |
|---|---|---|---|
| Device-nGnRnE | Device | Non-cacheable, no gather/reorder/early-ack | MMIO registers |
| Device-nGnRE | Device | Non-cacheable, no gather/reorder, early-ack | MMIO (most devices) |
| Device-GRE | Device | Non-cacheable, gather/reorder/early-ack | Framebuffer MMIO |
| Normal NC | Normal | Non-cacheable | DMA buffers |
| Normal WT | Normal | Write-through, read-allocate | Shared data |
| Normal WB | Normal | Write-back, read/write-allocate | General memory |

**Table 6.3:** Common memory type configurations. The T239's memory map assigns
these attributes to different physical address regions. [8]

### 6.4 Address Space Layout

The T239 physical address space is not publicly documented in full, but the
following can be inferred from the Orin T234 TRM and standard Tegra conventions:
[SPECULATIVE — Inferred from T234 TRM, no T239-specific documentation.] [11]

```
+------------------------------------------------------------------+
|                T239 Physical Address Space (Inferred)            |
|                                                                  |
|  0x0000_0000_0000 - 0x0000_7FFF_FFFF   Low memory (2 GB)       |
|  0x0000_8000_0000 - 0x0002_FFFF_FFFF   DRAM (up to 12 GB)      |
|  0x0003_0000_0000 - 0x0003_FFFF_FFFF   DRAM continued          |
|  0x0005_0000_0000 - 0x0005_0FFF_FFFF   GPU registers           |
|  0x0006_0000_0000 - 0x0006_0FFF_FFFF   CPU system registers    |
|  0x000A_0000_0000 - 0x000A_0FFF_FFFF   MMIO (peripherals)      |
+------------------------------------------------------------------+
```

**Figure 6.1:** T239 address space (SPECULATIVE). Based on T234 (Orin) TRM
and standard Tegra memory maps. Actual T239 addresses may differ. [11]

### 6.5 oboromi Memory Model

The oboromi emulator implements a flat 12 GB memory space starting at address
0x0, with stack allocation at the top of memory per core. [CONFIRMED — oboromi
source code.] [12]

```rust
// core/src/cpu/cpu_manager.rs
pub const MEMORY_SIZE: u64 = 12 * 1024 * 1024 * 1024; // 12 GB
pub const MEMORY_BASE: u64 = 0x0;
```

The UnicornCPU wrapper maps shared memory via `mem_map_ptr` with full read/
write/execute permissions, and initializes each core's stack pointer at the top
of memory with 1 MB spacing per core to avoid collisions. [CONFIRMED] [13]

---

## 7. Exception Levels & Security

### 7.1 Exception Level Architecture

The ARMv8-A architecture defines four exception levels (EL0–EL3), each
providing increasing privilege. [CONFIRMED — ARM Architecture Reference Manual.] [8]

```
+------------------------------------------------------------------+
|                Exception Level Architecture                       |
|                                                                  |
|  EL3 — Secure Monitor (highest privilege)                       |
|    |   - Switches between Secure and Non-secure state           |
|    |   - SCR_EL3.NS bit controls security state                 |
|    |                                                             |
|  EL2 — Hypervisor                                               |
|    |   - Virtualization of EL1/EL0                              |
|    |   - Stage 2 address translation                            |
|    |   - HCR_EL2 controls hypervisor behavior                   |
|    |                                                             |
|  EL1 — Operating System kernel                                  |
|    |   - Memory management (Stage 1 translation)                |
|    |   - Exception handling, interrupt routing                   |
|    |   - SCTLR_EL1 controls MMU, caches                         |
|    |                                                             |
|  EL0 — Application (lowest privilege)                           |
|        - User-space applications                                |
|        - Cannot access system registers                         |
|        - Uses SVC for system calls to EL1                       |
+------------------------------------------------------------------+
```

**Figure 7.1:** Exception levels. On Switch 2, Horizon OS runs at EL1/EL2,
with game code at EL0 and a secure monitor at EL3. [CONFIRMED] [8]

### 7.2 Exception Types

| Exception | Source | Target EL | Description |
|---|---|---|---|
| SVC (Supervisor Call) | EL0 → EL1 | EL1 | System call from user code |
| HVC (Hypervisor Call) | EL1 → EL2 | EL2 | Hypervisor call |
| SMC (Secure Monitor Call) | EL1/EL2 → EL3 | EL3 | Secure monitor call |
| IRQ (Interrupt Request) | External | Configurable | Interrupt |
| FIQ (Fast Interrupt) | External | Configurable | Fast interrupt |
| SError (System Error) | External | Configurable | Asynchronous error |
| Data Abort | Memory access | Same or higher EL | MMU fault |
| Instruction Abort | Fetch | Same or higher EL | Instruction fetch fault |
| BRK (Breakpoint) | Software | Same or higher EL | Debug breakpoint |
| UDF (Undefined) | Invalid instruction | Same or higher EL | Unimplemented instruction |

**Table 7.1:** Exception types. The A78C supports the full ARMv8-A exception
model including virtualization and TrustZone. [CONFIRMED] [8]

### 7.3 AArch32 Compatibility

The A78C supports AArch32 execution state at **EL0 only**. This allows legacy
32-bit applications to run, but the operating system and all system software
must use AArch64. The Nintendo Switch 2 SDK **does not support 32-bit** — all
game code must be AArch64. [CONFIRMED — Digital Foundry, A78C TRM.] [1][2]

---

## 8. Cryptographic Extensions

### 8.1 Crypto Extension Overview

The Cortex-A78C's Cryptographic Extension is a separately licensable product
that adds hardware-accelerated cryptographic instructions to the ASIMD unit.
The T239 **enables** the Cryptographic Extension per Nintendo SDK. [CONFIRMED
— Digital Foundry, A78C Crypto TRM.] [1][14]

### 8.2 Supported Crypto Instructions

| Algorithm | Instructions | Throughput |
|---|---|---|
| AES (encrypt/decrypt) | AESE, AESD, AESMC, AESIMC [CONFIRMED] | 1 cycle per round |
| AES (polynomial multiply) | PMULL, PMULL2 (64-bit) [CONFIRMED] | 1 cycle |
| SHA-1 | SHA1C, SHA1P, SHA1M, SHA1SU0, SHA1SU1 [CONFIRMED] | 1 cycle per op |
| SHA-256 | SHA256H, SHA256H2, SHA256SU0, SHA256SU1 [CONFIRM] | 1 cycle per op |
| CRC32 | CRC32B, CRC32H, CRC32W, CRC32X [CONFIRMED] | 1 cycle |
| Dot Product | SDOT, UDOT [CONFIRMED] | 1 cycle per vector |

**Table 8.1:** Cryptographic instructions. All operate on 128-bit NEON
registers. [CONFIRMED] [14]

### 8.3 Detection via ID Registers

Software can detect crypto support by reading: [CONFIRMED — A78C Crypto TRM.] [14]

- `ID_AA64ISAR0_EL1` (AArch64): AES[7:4], SHA1[11:8], SHA2[15:12], CRC32[19:16], DP[47:44]
- `ID_ISAR5_EL1` (AArch32): Same fields in 32-bit encoding

When CRYPTODISABLE is asserted at reset, all crypto instructions trap to
Undefined. [CONFIRMED] [14]

---

## 9. Interrupt Controller (GIC)

### 9.1 GIC Architecture

The A78C implements the **GICv3** (Generic Interrupt Controller version 3)
CPU interface, with optional **GICv4** support for direct injection of virtual
interrupts. [CONFIRMED — A78C TRM Table A1-3.] [2][8]

| Feature | Specification |
|---|---|
| GIC version | GICv3 / GICv4 [CONFIRMED] |
| Interrupt groups | Group 0 (FIQ), Group 1 Secure, Group 1 Non-secure [CONFIRMED] |
| Maximum SPIs | Up to 988 (implementation defined) [INFERRED] |
| PPIs per CPU | 32 private peripheral interrupts [CONFIRMED] |
| SGIs per CPU | 16 software-generated interrupts [CONFIRMED] |
| Priority levels | 256 (0–255, lower = higher priority) [CONFIRMED] |
| CPU interface | System register-based (ICC_*_ELx) [CONFIRMED] |

**Table 9.1:** GIC parameters. GICv3 uses system register access instead of
memory-mapped CPU interface registers. [2][8]

### 9.2 Key GIC System Registers

| Register | EL | Purpose |
|---|---|---|
| ICC_PMR_EL1 | EL1+ | Priority mask |
| ICC_IAR1_EL1 | EL1+ | Interrupt acknowledge (Group 1) |
| ICC_EOIR1_EL1 | EL1+ | End of interrupt (Group 1) |
| ICC_SRE_EL1 | EL1+ | System register enable |
| ICC_BPR1_EL1 | EL1+ | Binary point |
| ICC_CTLR_EL1 | EL1+ | Control |
| ICC_IGRPEN1_EL1 | EL1+ | Group 1 enable |

**Table 9.2:** GIC CPU interface registers. [CONFIRMED] [8]

---

## 10. Generic Timer

### 10.1 Timer Architecture

Each A78C core includes a set of timers based on a 64-bit system counter
distributed from the SoC. [CONFIRMED — A78C TRM A2.4.] [2][8]

| Timer | EL | Purpose |
|---|---|---|
| EL1 Physical Timer (CNTP) | EL1 | OS kernel scheduling |
| EL2 Physical Timer | EL2 | Hypervisor timer |
| EL3 Physical Timer | EL3 | Secure monitor timer |
| Virtual Timer (CNTV) | EL1 | Guest OS virtual timer |
| Hypervisor Virtual Timer | EL2 | Hypervisor virtual timer |
| System Counter | External | 64-bit, distributed to all cores |

**Table 10.1:** Generic Timer set. The system counter resides in the SoC (not
in the core) and provides a common time base. [CONFIRMED] [2]

### 10.2 Counter Frequency

The counter frequency is typically set at boot via `CNTFRQ_EL0`. The T239's
exact counter frequency is not publicly documented, but common Tegra
implementations use 19.2 MHz or 31.25 MHz. [SPECULATIVE — No T239-specific
documentation.] [11]

---

## 11. Clock Behavior & Power Management

### 11.1 CPU Clock Speeds

The T239 CPU complex operates at variable clock speeds depending on power mode:
[CONFIRMED — Digital Foundry, Nintendo developer documentation.] [1][3]

| Mode | CPU Clock | Notes |
|---|---|---|
| Handheld (mobile) | 1,101 MHz [CONFIRMED] | Higher than docked (counter-intuitive) |
| Docked (performance) | 998 MHz [CONFIRMED] | Lower than handheld |
| Maximum (boost) | 1,700 MHz [CONFIRMED] | Developer override / loading screen |

**Table 11.1:** CPU clock speeds. The handheld clock being higher than docked
is likely due to reduced memory bandwidth in handheld mode, where the CPU
compensates with higher frequency. [1][3]

### 11.2 Clock Gating

The A78C supports hierarchical clock gating for power efficiency:
[CONFIRMED — A78C TRM A3.] [2]

- **Architectural clock gating**: Automatic when units are idle
- **Dynamic Voltage and Frequency Scaling (DVFS)**: Per-core voltage/frequency
- **Power domains**: Core (PDCPU) and System (PDSYS) domains
- **Retention mode**: Dynamic retention for fast wake
- **Powerdown**: Full core powerdown with state save/restore

### 11.3 Power Modes

| Mode | Description | Wake Latency |
|---|---|---|
| Active | Full operation | — |
| Clock Gated | Clock stopped, state retained | 1–2 cycles |
| Retention | State preserved in retention flops | ~100 cycles |
| Powerdown | Full power-off, state lost | ~1000 cycles |

**Table 11.2:** Power modes. The OS dynamically manages power states based on
workload. [INFERRED — A78C TRM A4.]

---

## 12. Performance Characteristics

### 12.1 Single-Core Performance

Based on simulation of the T234 (Orin) A78AE cores downclocked to T239 speeds:
[INFERRED — Geekerwan benchmark analysis.] [5]

| Benchmark | T239 @ 1.1 GHz | Comparison |
|---|---|---|
| Geekbench 6 Single-Core | ~800–900 [INFERRED] | 2× PS4, 3× Switch 1 |
| Closest laptop equivalent | i7-4700HQ [INFERRED] | — |
| Closest mobile equivalent | Apple A12 (multi-core) [INFER] | — |

**Table 12.1:** Single-core performance estimates. [INFERRED] [5]

### 12.2 Multi-Core Performance

| Configuration | Estimated MT Performance | Notes |
|---|---|---|
| 8 cores @ 1.1 GHz | ~5000–6000 (GB6) [INFERRED] | All cores active |
| 6 developer cores | ~3800–4500 (GB6) [INFERRED] | 2 OS cores excluded |

**Table 12.2:** Multi-core performance estimates. [INFERRED] [5]

### 12.3 Memory Bandwidth Impact on CPU

The memory bandwidth asymmetry between modes affects CPU performance:
[CONFIRMED — Digital Foundry.] [1]

| Mode | Memory BW | Impact |
|---|---|---|
| Docked | 102 GB/s | Full bandwidth for CPU+GPU |
| Handheld | 68 GB/s | 33% less; CPU may compensate with higher clock |

**Table 12.3:** Memory bandwidth by mode. [CONFIRMED] [1]

---

## 13. Gap Analysis vs oboromi

### 13.1 Current oboromi CPU Implementation

The oboromi emulator currently implements a basic CPU manager using the Unicorn
Engine for ARM64 emulation. [CONFIRMED — oboromi source code.] [12][13]

| Component | oboromi Status | Gap |
|---|---|---|
| Core count | 8 cores ✅ [CONFIRMED] | None — matches T239 |
| Architecture | ARM64 (Arch::ARM64) ✅ [CONFIRMED] | Mode::LITTLE_ENDIAN correct |
| Memory model | 12 GB shared, flat @ 0x0 ✅ [CONFIRMED] | Real T239 has complex address map |
| Stack allocation | 1 MB per core at top of memory [CONFIRMED] | Simplified vs real memory layout |
| L1 Cache | Not implemented ❌ | No cache simulation |
| L2 Cache | Not implemented ❌ | No cache simulation |
| L3 Cache | Not implemented ❌ | No cache simulation |
| MMU/TLB | Not implemented ❌ | Unicorn handles translation internally |
| GIC | Not implemented ❌ | No interrupt controller |
| Generic Timer | Not implemented ❌ | No timer simulation |
| Crypto Extensions | Depends on Unicorn [INFERRED] | Unicorn may support crypto instructions |
| ASIMD/NEON | Depends on Unicorn [INFERRED] | Unicorn should handle NEON |
| Exception levels | Not implemented ❌ | Unicorn has no EL simulation |
| Cache coherency | Not implemented ❌ | No MESI protocol |
| DVFS/Clock gating | Not implemented ❌ | No power management |
| Pipeline simulation | Not implemented ❌ | No cycle-accurate simulation |
| Performance counters | Not implemented ❌ | No PMU |
| Debug infrastructure | Not implemented ❌ | No breakpoint/watchpoint support |
| Endianness | Little-endian ✅ [CONFIRMED] | Correct for T239 |

**Table 13.1:** Gap analysis. The oboromi CPU manager provides a functional
8-core ARM64 execution environment but lacks all hardware-specific features. [12][13]

### 13.2 Priority Gaps for Emulator Development

| Priority | Gap | Impact | Effort |
|---|---|---|---|
| P0 | Memory map (address space layout) | Required for MMIO, GPU registers | Medium |
| P0 | Cache simulation (at least timing) | Required for accurate performance | High |
| P1 | Interrupt controller (GICv3) | Required for OS boot, driver communication | High |
| P1 | Exception levels (EL0–EL3) | Required for OS/hypervisor separation | High |
| P1 | MMU with real translation tables | Required for virtual memory | High |
| P2 | Generic Timer | Required for OS scheduling | Medium |
| P2 | Crypto Extensions | Required for secure content | Low |
| P3 | Pipeline timing model | Required for cycle-accurate emulation | Very High |
| P3 | Power management | Required for power state transitions | Low |
| P3 | Debug infrastructure | Required for debugging tools | Medium |

**Table 13.2:** Priority gaps. P0 items block basic functionality; P1 items are
needed for OS boot; P2/P3 are needed for accuracy and compatibility.

### 13.3 Source File Reference

| File | Content | Lines |
|---|---|---|
| `core/src/cpu/cpu_manager.rs` | 8-core CPU manager, 12GB memory, round-robin execution | ~60 |
| `core/src/cpu/unicorn_interface.rs` | Unicorn Engine wrapper, ARM64 emulation, register/memory access | ~250 |

**Table 13.3:** oboromi CPU source files. Both files implement the current
minimal CPU emulation layer. [12][13]

---

## Citations

[1] Digital Foundry. "Nintendo Switch 2: final tech specs and system reservations
confirmed." May 2025. https://www.digitalfoundry.net/articles/digitalfoundry-2025-nintendo-switch-2-final-tech-specs-and-system-reservations-confirmed
Accessed: 2026-05-03.

[2] ARM. "Arm® Cortex®-A78C Core Revision: r0p2 Technical Reference Manual."
ARM DDI 0619. https://documentation-service.arm.com/static/6193c9bef45f0b1fbf3a85dd
Accessed: 2026-05-03.

[3] Gigazine. "A roundup of Nintendo Switch 2's unrevealed tech specs."
May 2025. https://gigazine.net/gsc_news/en/20250515-nintendo-switch-2-spec-detail/
Accessed: 2026-05-03.

[4] ARM. "Arm® Cortex®-A78 Core Revision: r1p1 Technical Reference Manual."
ARM DDI 0598. https://documentation-service.arm.com/static/5f159b6720b7cf4bc5247448
Accessed: 2026-05-03.

[5] Tom's Hardware / Geekerwan. "Nintendo Switch 2's SoC die shot reveals 8x
A78C cores, 1,536 Ampere shaders, and Samsung's 8nm process." May 2025.
https://www.tomshardware.com/pc-components/cpus/nintendo-switch-2s-soc-die-shot-reveals-8x-a78c-cores-1-536-ampere-shaders-and-samsungs-8n-process
Accessed: 2026-05-03.

[6] ARM Community. "Cortex-A78C: Enabling big-core compute for digital immersion."
2020. https://community.arm.com/arm-community-blogs/b/architectures-and-processors-blog/posts/arm-cortex-a78c
Accessed: 2026-05-03.

[7] ARM. "Arm® DynamIQ™ Shared Unit MP135 Technical Reference Manual."
Accessed: 2026-05-03.

[8] ARM. "Arm® Architecture Reference Manual for A-profile architecture."
Accessed: 2026-05-03.

[9] WikiChip. "Cortex-A78 - Microarchitectures - ARM." 2020.
https://en.wikichip.org/wiki/arm_holdings/microarchitectures/cortex-a78
Accessed: 2026-05-03.

[10] Wikipedia. "ARM Cortex-A78." 2025.
https://en.wikipedia.org/wiki/ARM_Cortex-A78
Accessed: 2026-05-03.

[11] NVIDIA. "Jetson Orin Technical Reference Manual (T234)." 2022.
Referenced as closest public documentation for T239 address space.
Accessed: 2026-05-03.

[12] oboromi source code. `core/src/cpu/cpu_manager.rs`. CONFIRMED.

[13] oboromi source code. `core/src/cpu/unicorn_interface.rs`. CONFIRMED.

[14] ARM. "Arm® Cortex®-A78 Core Cryptographic Extension." ARM DDI 0600.
https://documentation-service.arm.com/static/5f16a70620b7cf4bc52495f3
Accessed: 2026-05-03.

[15] Grainger School of CS. "CS 433 Mini-Project: ARM Cortex-A78." 2020.
https://courses.grainger.illinois.edu/cs433/fa2020/slides/mini-project-arm-cortex-a78.pdf
Accessed: 2026-05-03.

---

*Document generated: 2026-05-03*
*Last updated: 2026-05-03*
*Author: oboromi documentation system*
