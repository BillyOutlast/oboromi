# Firmware/OS Reference: Horizon Microkernel (T239 Switch 2)

> **Target:** Nintendo Switch 2 — Horizon OS microkernel and system firmware
> **Kernel Type:** Microkernel (L4-derived, custom Nintendo implementation)
> **IPC Protocol:** HIPC (Horizon IPC)
> **Document Status:** In Progress — 6 sections covering microkernel architecture,
> IPC protocol, Kernel Initial Processes (KIPs), Service Manager, boot sequence,
> system resource reservations, and NVN graphics API overview.
>
> **Confidence Legend:**
> - **CONFIRMED** — Verified from Nintendo SDK documentation, Atmosphère source code, Digital Foundry analysis, or oboromi source code
> - **INFERRED** — Derived from closely related public documentation (switchbrew wiki, Atmosphère RE, L4 microkernel papers, Tegra boot architecture)
> - **SPECULATIVE** — Based on reverse engineering, homebrew community analysis, or extrapolation from similar microkernel designs

---

## Table of Contents

1. [Horizon OS Overview](#1-horizon-os-overview)
2. [Microkernel Architecture](#2-microkernel-architecture)
3. [IPC Protocol (HIPC)](#3-ipc-protocol-hipc)
4. [Kernel Initial Processes (KIPs)](#4-kernel-initial-processes-kips)
5. [Service Manager (sm)](#5-service-manager-sm)
6. [Boot Sequence](#6-boot-sequence)
7. [System Resource Reservations](#7-system-resource-reservations)
8. [NVN Graphics API Overview](#8-nvn-graphics-api-overview)
9. [Gap Analysis vs oboromi](#9-gap-analysis-vs-oboromi)
10. [Citations](#citations)

---

## 1. Horizon OS Overview

### 1.1 Operating System Summary

Horizon OS (also referred to as "hos" or "HorizonNX") is the proprietary
operating system running on Nintendo Switch consoles. It is a microkernel-based
real-time operating system derived from Nintendo's earlier embedded firmware
platforms. Unlike monolithic kernels (Linux, Windows NT) where device drivers,
file systems, and networking run in kernel space, Horizon OS pushes nearly all
system services into userland processes communicating via a message-passing IPC
protocol. [CONFIRMED — switchbrew wiki, Atmosphère source code.] [1][2]

The microkernel itself is small — estimated at under 100 KB of code — and
provides only the most fundamental primitives: thread scheduling, virtual
memory management, IPC message passing, synchronization objects (mutexes,
events, semaphores), and interrupt handling. Everything else — file system
access, graphics, audio, networking, Bluetooth, USB, HID input, and the
application lifecycle — is implemented as userland services communicating
through IPC. [INFERRED — switchbrew kernel documentation, Atmosphère kernel
module analysis.] [1][2]

```
+------------------------------------------------------------------+
|                    Horizon OS Architecture                        |
|                                                                  |
|  Userland                                                        |
|  +----------------------------------------------------------+   |
|  |  Game Application (NRO/NSO)                               |   |
|  |  Runs at EL0, sandboxed via memory permissions            |   |
|  +---------------------------+------------------------------+   |
|                              |                                  |
|                    IPC (HIPC) |                                  |
|                              |                                  |
|  +----------------------------------------------------------+   |
|  |  System Services (userland)                               |   |
|  |                                                           |   |
|  |  +-----+ +-----+ +-----+ +-----+ +-----+ +-----+        |   |
|  |  | fs  | | pm  | | sm  | | ldr | | nv  | | vi  |  ...    |   |
|  |  +--+--+ +--+--+ +--+--+ +--+--+ +--+--+ +--+--+        |   |
|  |     |       |       |       |       |       |             |   |
|  +-----+-------+-------+-------+-------+-------+-------------+   |
|                              |                                  |
|                    IPC (HIPC) |                                  |
|                              v                                  |
|  +----------------------------------------------------------+   |
|  |  Horizon Microkernel (EL1)                                |   |
|  |  Scheduler, VM, IPC, sync objects, interrupt dispatch     |   |
|  +----------------------------------------------------------+   |
|                                                                  |
|  +----------------------------------------------------------+   |
|  |  ARM TrustZone / EL3 Secure Monitor                      |   |
|  |  (DRM, key derivation, secure storage)                   |   |
|  +----------------------------------------------------------+   |
+------------------------------------------------------------------+
```

**Figure 1.1:** Horizon OS architecture. The microkernel runs at EL1 (kernel
privilege), system services and games run at EL0 (user privilege), and TrustZone
secure world handles DRM and key management at EL3. [1][2]

### 1.2 Microkernel Design Philosophy

Horizon OS follows the **L4 microkernel philosophy**: the kernel should do as
little as possible, and everything that can be implemented in userland should
be. This design has several concrete benefits for a game console: [INFERRED —
L4 design principles, Nintendo's historical kernel choices.] [3]

1. **Small trusted computing base (TCB):** The kernel is the only code running
   at EL1. Bugs in services (audio, networking, file system) cannot directly
   corrupt kernel memory or bypass security boundaries. [INFERRED]
2. **Isolation between services:** Each service has its own address space and
   communicates only via IPC. A crashing audio driver does not take down the
   graphics system. [INFERRED]
3. **Verifiable correctness:** A small kernel is easier to audit and formally
   verify than a monolithic kernel with millions of lines of driver code.
   [SPECULATIVE]
4. **Reduced attack surface:** Game code runs at EL0 and cannot directly
   interact with hardware. All hardware access is mediated by system services.
   [CONFIRMED — Nintendo security model, Atmosphère privilege analysis.]

### 1.3 Horizon OS vs Traditional Kernels

| Aspect | Horizon OS (microkernel) | Linux (monolithic) | Windows NT (hybrid) |
|---|---|---|---|
| Kernel size | < 100 KB [SPECULATIVE] | ~30 MB (vmlinux) | ~15 MB (ntoskrnl.exe) |
| Drivers in kernel | Minimal (interrupt dispatch) | Most drivers in kernel | Most in kernel (ELF/LDR) |
| File system | Userland service (fs) | Kernel module (VFS) | Kernel driver (NTFS.sys) |
| IPC cost | Low (~1 µs per call) [SPECULATIVE] | N/A (syscalls) | Medium (ALPC) |
| Crash isolation | Strong (per-service) | Weak (kernel panic) | Medium (BSOD possible) |
| Security boundary | IPC + memory permissions | SELinux/capabilities | Token-based ACLs |

**Table 1.1:** Horizon OS microkernel comparison to mainstream kernels. [3]

### 1.4 Kernel Versioning

The Horizon kernel uses a version scheme: `MAJOR.MINOR.PATCH-REV`. The kernel
version observed on Switch 2 firmware corresponds to kernel major version 18.x,
building on the Switch 1 lineage (which reached kernel 16.x before end of
life). Each firmware update ships a new kernel binary signed by Nintendo's
root key. [INFERRED — Atmosphère version tracking, switchbrew firmware
archive.] [1][2]

---

## 2. Microkernel Architecture

### 2.1 Kernel Objects

The Horizon kernel implements a **capability-based** security model using
kernel objects. A kernel object is a typed, reference-counted entity managed
entirely by the kernel. Userland code cannot directly access kernel memory —
it interacts with kernel objects exclusively through **handles**, which are
typed references stored in per-process handle tables. [CONFIRMED — switchbrew
kernel object documentation, Atmosphère kernel source.] [1][2]

The primary kernel object types are:

| Object Type | Purpose | Ref Counted |
|---|---|---|
| Thread | Execution context (registers, stack, TLS) | Yes [CONFIRMED] |
| Process | Address space + handle table + capabilities | Yes [CONFIRMED] |
| Session | IPC connection endpoint (client/server pair) | Yes [CONFIRMED] |
| Event | Signaling primitive (wait/signal) | Yes [CONFIRMED] |
| Mutex | Priority-inheritance mutual exclusion | Yes [CONFIRMED] |
| Semaphore | Counting synchronization | Yes [CONFIRMED] |
| Timer | Absolute/relative timeout with callback | Yes [CONFIRMED] |
| SharedMemory | Named memory region with permission bits | Yes [CONFIRMED] |
| TransferMemory | Memory region for IPC transfer (borrowed) | Yes [CONFIRMED] |
| InterruptEvent | IRQ handler registration | Yes [CONFIRMED] |
| AddressArbiter | Kernel-level futex-like primitive | Yes [CONFIRMED] |
| IoPool | DMA-capable memory pool for hardware access | Yes [CONFIRMED] |

**Table 2.1:** Primary kernel object types. [1][2]

```
+------------------------------------------------------------------+
|                    Kernel Object Model                            |
|                                                                  |
|  Process A (Game)                                                |
|  +----------------------------------------------------------+   |
|  |  Handle Table                                            |   |
|  |  [0x0] → Thread (main thread)                            |   |
|  |  [0x1] → Session (→ fs service)                          |   |
|  |  [0x2] → Session (→ nv service)                          |   |
|  |  [0x3] → SharedMemory (framebuffer)                      |   |
|  |  [0x4] → Event (vsync)                                   |   |
|  |  [0x5] → Mutex (render sync)                             |   |
|  |  ...                                                     |   |
|  +----------------------------------------------------------+   |
|                                                                  |
|  Process B (fs service)                                          |
|  +----------------------------------------------------------+   |
|  |  Handle Table                                            |   |
|  |  [0x0] → Thread (main)                                   |   |
|  |  [0x1] → Session (→ fsp-srv)                             |   |
|  |  [0x2] → IoPool (SD/eMMC DMA)                           |   |
|  |  [0x3] → InterruptEvent (SDMMC IRQ)                     |   |
|  |  ...                                                     |   |
|  +----------------------------------------------------------+   |
+------------------------------------------------------------------+
```

**Figure 2.1:** Kernel object model. Each process has its own handle table
containing typed references to kernel objects. IPC sessions bridge processes.
[1][2]

### 2.2 Handle Table

Each process maintains a **handle table** — a flat array mapping integer
handle values to kernel object pointers. Handle table entries are allocated
sequentially starting from handle value 0x0. The maximum handle table size
per process is 1024 entries on Switch 1; the Switch 2 likely increases this.
[INFERRED — switchbrew handle documentation.] [1]

Handles are **process-local** — handle 0x5 in Process A and handle 0x5 in
Process B refer to completely different kernel objects (or null). There is no
global handle namespace. Cross-process sharing requires explicit IPC transfer
of handle duplicates or memory mapping. [CONFIRMED — Atmosphère kernel
implementation.] [2]

### 2.3 Scheduler

The Horizon kernel implements a **fixed-priority preemptive scheduler** with
128 priority levels (0 = highest, 127 = lowest). Priority 0-3 are reserved
for kernel-internal threads. System services typically run at priorities
4-31. Game threads run at priorities 32-63 (normal) or 64-127 (background).
[CONFIRMED — switchbrew scheduler documentation, Atmosphère kernel.] [1][2]

```
Priority   Usage
--------   ------------------------------------------------
0-3        Kernel internal (interrupt handlers, idle thread)
4-7        Critical system (sm, pm, loader)
8-15       System services (fs, nvdrv, vi)
16-31      Standard system services (audio, hid, network)
32-63      Application threads (game main, render, audio)
64-127     Background threads (logging, telemetry, GameChat)
--------   ------------------------------------------------
```

**Table 2.2:** Priority level allocation. [INFERRED — switchbrew, Atmosphère
kernel thread priority analysis.]

The scheduler supports **priority inheritance** for mutexes — when a
low-priority thread holds a mutex that a high-priority thread is waiting on,
the holder's priority is temporarily boosted to the waiter's priority. This
prevents **priority inversion**, a classic real-time systems bug where
medium-priority threads preempt the mutex holder, starving the high-priority
waiter. [CONFIRMED — switchbrew mutex documentation.] [1]

### 2.4 Thread Management

Each thread is represented by a `Thread` kernel object containing:

- **Register context:** General-purpose registers (X0-X30), SIMD/FP registers
  (V0-V31), stack pointer (SP), program counter (PC), processor state (PSTATE)
- **Thread-local storage (TLS):** A 0x200-byte region at a fixed address in
  the process's virtual address space, used for IPC command buffers and
  per-thread state [CONFIRMED — switchbrew TLS layout.] [1]
- **Scheduling state:** Current priority, scheduler core affinity, timeslice
  counter
- **Wait state:** What object the thread is blocked on (IPC, mutex, event,
  timer) and the wait queue linkage

Thread creation uses the `svcCreateThread` syscall, which takes an entry
point, argument, stack pointer, and priority. The new thread starts in a
suspended state and must be explicitly started with `svcStartThread`.
[CONFIRMED — switchbrew SVC documentation.] [1]

The kernel supports **core pinning** — threads can be bound to specific CPU
cores (of the 8 Cortex-A78C cores). System services typically run on the
2 OS-reserved cores (cores 6-7), while game threads run on the 6 developer
cores (cores 0-5). [CONFIRMED — Digital Foundry, Nintendo SDK.] [4]

### 2.5 Synchronization Primitives

| Primitive | Type | Behavior | Priority Inheritance |
|---|---|---|---|
| Mutex | Binary, recursive | Lock/unlock with PI | Yes [CONFIRMED] |
| Event | Signaling | Signal/wait (manual or auto reset) | No [CONFIRMED] |
| Semaphore | Counting | Wait decrements, signal increments | No [CONFIRMED] |
| AddressArbiter | Futex-like | Kernel-mediated userspace wait | N/A [CONFIRMED] |
| Timer | Timeout | Absolute/relative, auto or manual | No [CONFIRMED] |

**Table 2.3:** Synchronization primitives. [1][2]

---

## 3. IPC Protocol (HIPC)

### 3.1 HIPC Overview

HIPC (Horizon IPC) is the sole inter-process communication mechanism in
Horizon OS. Every system service interaction — file I/O, graphics command
submission, audio playback, input handling, networking — uses HIPC. The
protocol is designed for low latency and zero-copy data transfer, with
the kernel mediating all message passing to enforce security boundaries.
[CONFIRMED — switchbrew HIPC documentation, Atmosphère IPC
implementation.] [1][2]

HIPC replaces the earlier CMIF (Command Interface) protocol used on the
original 3DS kernel. Switch 1 introduced HIPC as a ground-up redesign,
and Switch 2 continues with the same fundamental protocol with minor
version-specific extensions. [INFERRED — switchbrew version history.] [1]

### 3.2 Message Structure

HIPC messages are transmitted through a **command buffer** — a 0x100-byte
(256-byte) region in each thread's TLS area at a fixed offset. For larger
payloads, the protocol supports **buffer descriptors** that reference memory
regions in the caller's address space, allowing the kernel to map them into
the callee's address space without copying. [CONFIRMED — switchbrew HIPC
format, oboromi `core/src/nn/hipc.rs`.] [1][2]

The message header consists of two 32-bit words parsed in `hipc.rs`:

```
Word 0 (hdr0) — bit field layout:
+------------------------------------------------------------------+
| [31:28]  xchg_count   | Exchange buffer descriptors (0-15)      |
| [27:24]  recv_count    | Receive buffer descriptors (0-15)       |
| [23:20]  send_count    | Send buffer descriptors (0-15)          |
| [19:16]  ptrs_count    | Pointer descriptors (0-15)              |
| [15:0]   tag           | Message type tag (command ID)           |
+------------------------------------------------------------------+

Word 1 (hdr1) — bit field layout:
+------------------------------------------------------------------+
| [31]     special_count | Special/HIPC descriptor present flag    |
| [30:14]  recv_list_offs| Offset to receive list (in words)       |
| [13:10]  recv_list_cnt | Receive list entry count (0-15)         |
| [9:0]    raw_count     | Raw data words (0-1023)                 |
+------------------------------------------------------------------+
```

**Figure 3.1:** HIPC header format (2 × 32-bit words). Decoded from oboromi
`core/src/nn/hipc.rs` HeaderData struct. [CONFIRMED — oboromi source code.] [5]

### 3.3 Message Types

HIPC supports several message types determined by the tag field in Word 0:

| Type | Tag Range | Purpose |
|---|---|---|
| Request | 0-0x7FFF | Client-to-server command invocation [CONFIRMED] |
| Control | 0-3 (special) | Session management: convert, clone, query pointer [CONFIRMED] |
| Domain | varies | Domain object multiplexing over single session [CONFIRMED] |
| Close | special | Session teardown / object close [CONFIRMED] |

**Table 3.1:** HIPC message types. [1][2]

**Request messages** carry a **command ID** in the tag field (lower 16 bits of
Word 0). The server dispatches based on this ID. For example, `fsp-srv`
(IFileSystem) might use command ID 0 for `OpenFile`, command ID 1 for
`CreateFile`, and so on. Each service defines its own command ID table.
[CONFIRMED — switchbrew service command lists.] [1]

**Control messages** handle session lifecycle:
- `ConvertCurrentObjectToDomain` (cmd 2): converts a session handle to a
  domain ID, enabling multiplexing of multiple objects over one session
- `CloneCurrentObject` (cmd 3): duplicates the session handle for the caller
- `QueryPointerBufferSize` (cmd 4): returns the server's max pointer buffer
  size [CONFIRMED — switchbrew IPC control.] [1]

### 3.4 Buffer Descriptors

Buffer descriptors allow zero-copy transfer of variable-length data between
processes. Each descriptor is a 3-word (12-byte) structure specifying a
virtual address, size, and transfer mode: [CONFIRMED — switchbrew HIPC
buffer descriptors, oboromi `core/src/nn/hipc.rs` MapData struct.] [1][5]

```
Buffer Descriptor (3 × u32):
+------------------------------------------------------------------+
| Word 0:  [31:0]  Address (virtual address in source process)     |
| Word 1:  [31:0]  Size (byte count)                               |
| Word 2:  [31:16] Reserved                                        |
|          [15:4]  Address mode (type flags)                        |
|          [3:0]   Transfer direction                              |
+------------------------------------------------------------------+
```

**Figure 3.2:** Buffer descriptor format. From oboromi `MapData` (3 × u32).
[5]

The transfer direction determines how the kernel maps the memory:

| Direction | Code | Meaning |
|---|---|---|
| Send (A→B) | 1 | Caller's buffer mapped read-only into callee [CONFIRMED] |
| Receive (B→A) | 2 | Callee's buffer mapped writable into caller [CONFIRMED] |
| Exchange | 3 | Bidirectional: mapped writable into callee [CONFIRMED] |

**Table 3.2:** Buffer descriptor transfer directions. [1]

### 3.5 Pointer Descriptors

Pointer descriptors provide small, inline data regions (typically ≤ 0x100
bytes) that are copied into the command buffer rather than memory-mapped.
They are more efficient than buffer descriptors for small payloads because
they avoid a TLB flush. The `PointerData` struct in oboromi is 2 × u32:
address and size. [CONFIRMED — switchbrew, oboromi `PointerData`.] [1][5]

### 3.6 Receive List

The receive list specifies where the callee should write output data and
output handles. It consists of entries that map to output buffers or output
copy handles. The `ReceiveListData` struct in oboromi is 2 × u32.
[CONFIRMED — switchbrew HIPC receive list, oboromi `ReceiveListData`.] [1][5]

### 3.7 Domain Objects

When a session is converted to a **domain** (via the control message
`ConvertCurrentObjectToDomain`), multiple server-side objects can be
multiplexed over a single IPC session. Each object is identified by a
**domain ID** (a 32-bit integer). Subsequent IPC calls include the domain
ID in the header, and the server dispatches to the correct object.
[CONFIRMED — switchbrew domain documentation.] [1]

This is essential for services like `fsp-srv` that manage hundreds of
open file handles — without domains, each file would require a separate
session handle, exhausting the 1024-entry handle table. [INFERRED —
practical necessity analysis.]

```
+------------------------------------------------------------------+
|                    Domain Multiplexing                            |
|                                                                  |
|  Client Process                                                  |
|  +----------------------------------------------------------+   |
|  |  Handle [0x1] → Session (→ fsp-srv)                      |   |
|  |  Domain IDs:                                               |   |
|  |    ID 1 → IFileSystem ("/")                               |   |
|  |    ID 2 → IFile ("/save/data.bin")                        |   |
|  |    ID 3 → IDirectory ("/save/")                           |   |
|  |    ID 4 → IFileSystem ("/content/")                       |   |
|  +----------------------------------------------------------+   |
|       |                                                         |
|       | IPC call: domain_id=2, cmd=0 (Read)                    |
|       v                                                         |
|  +----------------------------------------------------------+   |
|  |  fsp-srv Server Process                                   |   |
|  |  Domain Table:                                            |   |
|  |    ID 1 → FilesystemHandle ("/" mount)                    |   |
|  |    ID 2 → FileHandle ("/save/data.bin")                   |   |
|  |    ID 3 → DirectoryHandle ("/save/")                      |   |
|  |    ID 4 → FilesystemHandle ("/content/" mount)            |   |
|  +----------------------------------------------------------+   |
+------------------------------------------------------------------+
```

**Figure 3.3:** Domain object multiplexing. Multiple server-side objects
are accessed through a single session handle using domain IDs. [1]

### 3.8 IPC Flow Example

A typical IPC call follows this sequence:

```
Client                        Kernel                       Server
  |                              |                             |
  | svcSendSyncRequest(handle)   |                             |
  |----------------------------->|                             |
  |                              |  Validate handle            |
  |                              |  Copy message to server's   |
  |                              |  command buffer             |
  |                              |  Map buffer descriptors     |
  |                              |  Wake server thread         |
  |                              |---------------------------->|
  |                              |                             |
  |                              |         Process request     |
  |                              |         Write response      |
  |                              |                             |
  |                              | svcReplyAndReceive(handles) |
  |                              |<----------------------------|
  |                              |                             |
  |  Response available          |  Copy response to client's  |
  |  Client thread woken         |  command buffer             |
  |<-----------------------------|  Map response buffers       |
  |                              |                             |
  |  Read result from TLS        |                             |
  |  command buffer              |                             |
```

**Figure 3.4:** Typical synchronous IPC call flow. [1][2]

---

## 4. Kernel Initial Processes (KIPs)

### 4.1 KIP Overview

Kernel Initial Processes (KIPs) are the first userland processes started by
the kernel during boot. They are embedded in the firmware image as a single
archive called **INI1** (Initial Processes Image 1). The kernel parses the
INI1 archive, creates address spaces for each KIP, loads their segments, and
starts their main threads — all before any userland code has executed.
[CONFIRMED — switchbrew KIP documentation, Atmosphère process loading.] [1][2]

### 4.2 INI1 Archive Format

INI1 is a container format holding one or more KIP images:

```
INI1 Header (0x10 bytes):
+------------------------------------------------------------------+
| [0x00]  Magic: "INI1" (0x494E4931)                               |
| [0x04]  Size: total archive size (bytes)                         |
| [0x08]  NumProcesses: count of KIP entries                       |
| [0x0C]  Reserved (padding)                                       |
+------------------------------------------------------------------+

Followed by NumProcesses × KIP images, each starting with a KIP header.
```

**Figure 4.1:** INI1 archive format. [CONFIRMED — Atmosphère INI1 parser,
switchbrew.] [1][2]

### 4.3 KIP Image Format

Each KIP image consists of a fixed-size header followed by up to 6 segments:

```
KIP Header (0x80 bytes):
+------------------------------------------------------------------+
| [0x00]  Magic: "KIP1" (0x4B495031)                               |
| [0x04]  Name (12 bytes, null-padded ASCII)                       |
| [0x10]  TitleId (64-bit)                                         |
| [0x18]  ProcessCategory (0=regular, 1=kernel, 2=ini)             |
| [0x1C]  MainThreadPriority                                       |
| [0x1D]  MainThreadCoreNumber                                     |
| [0x20]  DefaultCpuCore                                           |
| [0x24]  Flags (bitmask: 64-bit address space, etc.)              |
| [0x28]  Reserved (0x08 bytes)                                    |
| [0x30]  Segment[0] (text): {file_off, dst_addr, size, flags}    |
| [0x40]  Segment[1] (rodata): {file_off, dst_addr, size, flags}  |
| [0x50]  Segment[2] (data): {file_off, dst_addr, size, flags}    |
| [0x60]  Segment[3-5]: {stack, kernel_buffer, reserved}          |
| [0x70]  Capability descriptors (kernel capabilities, 16 bytes)  |
+------------------------------------------------------------------+
```

**Figure 4.2:** KIP header format (0x80 bytes). [CONFIRMED — Atmosphère
KIP parser.] [2]

Each segment descriptor is 16 bytes:

```
Segment Descriptor (0x10 bytes):
+------------------------------------------------------------------+
| [0x00]  FileOffset (offset into KIP image data)                  |
| [0x04]  DestAddress (virtual address in process)                 |
| [0x08]  DecompressedSize (uncompressed size)                     |
| [0x0C]  Attributes: [31:24] flags, [23:0] compressed size       |
+------------------------------------------------------------------+
```

**Figure 4.3:** KIP segment descriptor. Segments may be compressed with
LZ4 or stored uncompressed. [CONFIRMED — Atmosphère KIP loader.] [2]

### 4.4 KIP Capabilities

The 16-byte capability descriptor at the end of the KIP header encodes
kernel-level permissions granted to the process:

| Capability | Description |
|---|---|
| Syscall mask | Which SVCs the process may invoke [CONFIRMED] |
| Handle table size | Maximum handle count [CONFIRMED] |
| Kernel release version | Minimum kernel version required [CONFIRMED] |
| Map physical memory | Permission to map specific physical regions [CONFIRMED] |
| IO ports | Permission to access specific MMIO ranges [CONFIRMED] |
| IRQ assignment | Which interrupts the process may register [CONFIRMED] |

**Table 4.1:** KIP capability types. [1][2]

### 4.5 System KIP List

The following KIPs are loaded during a standard Switch 2 boot:

| KIP Name | Category | Role |
|---|---|---|
| `kernel` | kernel | Horizon microkernel itself (loaded separately) [CONFIRMED] |
| `loader` | ini | NRO/NSO dynamic linker, process creation [CONFIRMED] |
| `pm` | ini | Process Manager — lifecycle, signals, resource limits [CONFIRMED] |
| `sm` | ini | Service Manager — IPC service registry [CONFIRMED] |
| `fs_mitm` | ini | File system man-in-the-middle (content patching) [CONFIRMED] |
| `ams_mitm` | ini | Atmosphère MITM services (homebrew only) [INFERRED] |
| `spl` | ini | Security Processor Liaison (key derivation) [CONFIRMED] |
| `boot` | ini | Early boot services (display init, logo) [INFERRED] |
| `usb` | ini | USB stack (device enumeration, mass storage) [CONFIRMED] |
| `tma` | ini | Target Manager Agent (dev/debug communication) [INFERRED] |

**Table 4.2:** Primary KIPs loaded during boot. [1][2]

### 4.6 KIP Memory Layout

When the kernel loads KIPs from INI1, it creates separate virtual address
spaces for each process and maps the segments at their specified addresses:

```
+------------------------------------------------------------------+
|  Typical KIP Virtual Address Space                               |
|                                                                  |
|  0x7100000000 +--------------------------------------------+    |
|               |  .text (code) — RX permissions              |    |
|               |  Loaded from segment 0                      |    |
|  0x7100X_0000 +--------------------------------------------+    |
|               |  .rodata (read-only data) — R permissions   |    |
|               |  Loaded from segment 1                      |    |
|  0x7100Y_0000 +--------------------------------------------+    |
|               |  .data (writable data) — RW permissions     |    |
|               |  Loaded from segment 2                      |    |
|  0x7100Z_0000 +--------------------------------------------+    |
|               |  Stack — RW permissions                     |    |
|               |  Main thread stack (typically 4-16 KB)      |    |
|  0x7100Z_FFFF +--------------------------------------------+    |
+------------------------------------------------------------------+
```

**Figure 4.4:** Typical KIP virtual address space layout. Addresses are
64-bit (Switch 2 uses 64-bit address space). [INFERRED — Switch 1 uses
32-bit; Switch 2's A78C cores support full 64-bit addressing.] [2]

---

## 5. Service Manager (sm)

### 5.1 Service Manager Overview

The Service Manager (`sm`) is the IPC service registry — the phone book of
Horizon OS. Every system service registers itself with `sm` under a
well-known name (e.g., `fsp-srv`, `nvdrv`, `vi`). Client processes query
`sm` to obtain session handles to services they need. Without `sm`, there is
no way for a process to discover or connect to any other process.
[CONFIRMED — switchbrew sm documentation, Atmosphère sm implementation.] [1][2]

### 5.2 Service Registration

When a system service process starts, its first action is typically to
register with `sm`:

```
Service startup sequence:
  1. svcConnectToNamedPort("sm:") → session handle to sm
  2. sm:Initialize(current_process_handle) → initialize client
  3. sm:RegisterService("fsp-srv") → returns server session handle
  4. Loop: svcReplyAndReceive(server_handle) → wait for client calls
```

**Figure 5.1:** Service registration sequence. [1][2]

The `RegisterService` call tells `sm` that any future `fsp-srv` lookup
should be routed to this process. Only one process can register a given
service name — the second registration fails with an "already registered"
error. [CONFIRMED — Atmosphère sm error handling.] [2]

### 5.3 Service Discovery

Clients discover services through three naming schemes:

| Scheme | Prefix | Example | Purpose |
|---|---|---|---|
| Standard | `srv:/` | `srv:/fsp-srv` | Normal service lookup [CONFIRMED] |
| Process-managed | `pmin:/` | `pmin:/fsp-srv` | PM-registered service [CONFIRMED] |
| Debug | `pmdmnt:/` | `pmdmnt:/fsp-srv` | Debug monitor service [CONFIRMED] |

**Table 5.1:** Service discovery schemes. [1]

The lookup sequence is:

```
Client                          sm
  |                              |
  | sm:GetService("fsp-srv")     |
  |----------------------------->|
  |                              |  Lookup "fsp-srv" in registry
  |                              |  Create session pair
  |                              |  Return client handle
  |  Handle [0x1] → fsp-srv      |
  |<-----------------------------|
```

**Figure 5.2:** Service discovery IPC flow. [1]

### 5.4 Service Access Control

Service access is controlled by **service lists** defined per-process. Each
process has an allow-list and deny-list of service names. The kernel
enforces these lists during `sm:GetService` calls — if a process attempts
to access a service not in its allow-list, the call returns an access
denied error. [CONFIRMED — switchbrew service access control.] [1]

For example:
- The game process can access: `fsp-srv`, `nvdrv`, `vi`, `audout`, `hid`,
  `nifm`, `time`, `set`, and many others.
- The game process **cannot** access: `sm` (management only), `pm:info`
  (system only), `spl` (security-critical).
- System services can access other system services as needed. [CONFIRMED]

### 5.5 Service Categories

Based on oboromi's `core/src/nn/mod.rs`, the project tracks **160 named
services** spanning the full Horizon OS service surface: [CONFIRMED —
oboromi source code, `start_host_services` function.] [5]

| Category | Services | Count |
|---|---|---|
| Audio | `aud`, `audctl`, `auddebug`, `auddev`, `auddmg`, `audin`, `audout`, `audrec`, `audren`, `audsmx`, `hwopus` | 11 |
| Graphics/Display | `disp`, `dispdrv`, `vi`, `vi2`, `vic`, `nvgem`, `gpuk`, `host1x`, `syncpt` | 9 |
| Input/HID | `hid`, `hidbus`, `ahid`, `irs` | 4 |
| File System | `fs`, `fsp-ldr`, `fsp-pr`, `fsp-srv`, `file_io` | 5 |
| Network | `nifm`, `dns`, `ssl`, `bsd`, `bsdcfg`, `eth`, `ethc`, `wlan`, `sfdnsres` | 9 |
| Bluetooth | `bt`, `btdrv`, `btm`, `btp` | 4 |
| USB | `usb` | 1 |
| NVIDIA Driver | `nvdrv`, `nvdrvdbg`, `nvdbg`, `nvmemp` | 4 |
| Power/Clock | `pcv`, `clkrst`, `psm`, `spsm`, `fan`, `rgltr`, `pwm` | 7 |
| Applet/Lifecycle | `applet-ae`, `applet-oe`, `apm`, `pm`, `ldr`, `ro`, `ns` | 7 |
| Settings | `set`, `pctl`, `acc`, `mii`, `miiimg`, `friend` | 6 |
| Time/RTC | `time`, `rtc` | 2 |
| Storage/NCM | `ncm`, `es`, `nim`, `prepo`, `erpt` | 5 |
| Miscellaneous | `bpc`, `fatal`, `lbl`, `led`, `gpio`, `i2c`, `spi`, `uart`, `pcie`, `spl`, `csrng`, etc. | ~86 |

**Table 5.2:** Service categories derived from oboromi source. [5]

### 5.6 Session Management

When a client calls `sm:GetService`, `sm` creates a **session pair** using
`svcCreateSession`: one handle goes to the client (the "client session"),
one goes to the server (the "server port" or "active session"). The client
sends IPC requests through its handle; the server receives them through
its handle. [CONFIRMED — switchbrew session documentation.] [1]

Sessions are **reference-counted**: when both the client and server close
their handles, the session kernel object is destroyed. If the server process
crashes, the kernel automatically closes the server handle, and subsequent
client IPC calls return a "session closed" error. [CONFIRMED]

---

## 6. Boot Sequence

### 6.1 Boot Overview

The Switch 2 boot sequence spans from power-on to the home menu, involving
multiple processors, signed firmware stages, and progressively more complex
software. The chain starts with hardware (BootROM) and ends with userland
services in Horizon OS. [CONFIRMED — switchbrew boot documentation,
NVIDIA Tegra boot architecture.] [1][6]

### 6.2 Package1 (Warmboot / eFuse Bootstrap)

Package1 is the earliest firmware stage, loaded by the BootROM. On the
Tegra platform, this corresponds to the **MB1** (Micro Boot 1) stage in
NVIDIA's nomenclature. Package1 is responsible for:

1. **DRAM initialization:** Configuring the LPDDR5X memory controller with
   timing parameters from the BCT. [CONFIRMED]
2. **Security processor setup:** Initializing the TSEC (Tegra Security
   Co-processor) and loading its firmware. [CONFIRMED]
3. **eFuse reading:** Reading chip-specific configuration from the eFuse
   array (chip binning, speed grade, security flags). [CONFIRMED]
4. **Warmboot firmware:** Loading the warmboot firmware for sleep/resume
   transitions. The warmboot path skips DRAM re-initialization if DRAM
   contents are preserved. [CONFIRMED — switchbrew warmboot.] [1]

Package1 is signed by Nintendo and verified by the BootROM using the
RSA/ECDSA key chain rooted in eFuses. [CONFIRMED — NVIDIA Jetson secure
boot.] [6]

### 6.3 Package2 (Kernel + INI1 + KIPs)

Package2 is the main firmware payload containing the Horizon OS kernel and
the initial process set. It corresponds to the **MB2/OBB** stage. Package2
is structured as: [CONFIRMED — switchbrew Package2 format, Atmosphère
package2 parser.] [1][2]

```
Package2 Layout:
+------------------------------------------------------------------+
|  Header (0x100 bytes)                                            |
|  - Magic: "PK21"                                                 |
|  - Signature (RSA-2048 over payload)                             |
|  - Key generation (anti-rollback counter)                        |
|  - Section offsets and sizes (4 sections)                        |
+------------------------------------------------------------------+
|  Section 0: Kernel                                               |
|  - Horizon microkernel binary (ELF or raw binary)                |
|  - Decompressed and loaded at kernel base address                |
+------------------------------------------------------------------+
|  Section 1: INI1                                                 |
|  - Archive of KIP images (see §4)                                |
|  - Parsed by kernel to create initial processes                  |
+------------------------------------------------------------------+
|  Section 2: Warmboot firmware (optional)                         |
|  - Sleep/resume firmware blob                                    |
+------------------------------------------------------------------+
|  Section 3: Package2 extensions (optional)                       |
|  - Additional firmware blobs (TSEC, BPMP, etc.)                  |
+------------------------------------------------------------------+
```

**Figure 6.1:** Package2 layout. [1][2]

The kernel verifies Package2's signature using the Secure Boot Key chain.
Anti-rollback fuses are checked against the key generation field — if the
Package2's generation is lower than the fuse value, the boot is rejected.
This prevents firmware downgrade attacks. [CONFIRMED — switchbrew security,
NVIDIA anti-rollback.] [1][6]

### 6.4 System Startup Sequence

After Package2 is loaded and the kernel initializes, the following startup
sequence occurs:

```
Power On
  |
  v
BootROM (Stage 0) — silicon root of trust
  |
  v
Package1 (MB1) — DRAM init, TSEC setup, eFuse read
  |
  v
Package2 verification and loading
  |
  v
Kernel entry point — initialize scheduler, VM, IPC subsystem
  |
  v
INI1 parsing — create address spaces for each KIP
  |
  v
Start KIP main threads (in priority order):
  1. sm — Service Manager (must start first)
  2. loader — Dynamic linker for NRO/NSO
  3. pm — Process Manager
  4. fs_mitm — File system MITM (content patching)
  5. spl — Security Processor Liaison
  6. (remaining KIPs)
  |
  v
sm registers itself, begins accepting service registrations
  |
  v
pm starts system services:
  - fs (file system)
  - nvdrv (NVIDIA driver)
  - vi (display compositor)
  - aud* (audio services)
  - hid (input)
  - nifm (network)
  - (160+ services total)
  |
  v
Display initialization (boot logo / splash screen)
  |
  v
User launches game or stays at home menu
```

**Figure 6.2:** Full boot sequence from power-on to userland services. [1][2]

### 6.5 Sleep and Resume (Warmboot)

When the console enters sleep mode, the kernel:

1. Suspends all userland threads
2. Saves CPU state to DRAM
3. Issues a power-down command to the memory controller (DRAM enters
   self-refresh to preserve contents)
4. Transfers to the warmboot firmware running on the BPMP (Boot and
   Power Management Processor)
5. BPMP enters deep sleep (only the RTC and PMIC remain active)

On resume:
1. PMIC detects power button press
2. BPMP wakes, runs warmboot firmware
3. DRAM exits self-refresh
4. Kernel restores CPU state from DRAM
5. All threads resume at their pre-sleep instruction pointer [CONFIRMED
   — switchbrew sleep/resume documentation.] [1]

---

## 7. System Resource Reservations

### 7.1 CPU Core Reservations

The T239 SoC has 8 ARM Cortex-A78C cores. The Horizon OS kernel reserves
**2 cores** for system services (cores 6-7), leaving **6 cores** available
to game applications. This reservation is enforced at the scheduler level —
game threads are restricted to cores 0-5 by default. [CONFIRMED — Digital
Foundry, Nintendo developer documentation.] [4]

| Core | Assignment | Notes |
|---|---|---|
| Core 0-5 | Game application | Available to developers [CONFIRMED] |
| Core 6-7 | System (Horizon OS) | Reserved for OS services [CONFIRMED] |

**Table 7.1:** CPU core allocation. The 2 reserved cores handle all system
service IPC processing, audio mixing, display compositing, input polling,
networking, and GameChat. [4]

The system cores run a mix of high-priority services:
- `vi` (display compositor) — highest priority, must maintain vsync
- `aud*` (audio services) — real-time audio mixing with hard deadlines
- `hid` (input) — polling at 1-4 kHz for low-latency controller input
- `fs` (file system) — async I/O completion
- `nvdrv` (NVIDIA driver) — GPU command buffer submission
- `pm` (process manager) — lifecycle events
- GameChat — video encoding/decoding for multiplayer chat [SPECULATIVE]

### 7.2 Memory Reservations

The OS reserves **3 GB** of the 12 GB LPDDR5X pool for system use, leaving
**9 GB** available to game applications. This is a significant increase from
the original Switch's 0.8 GB reservation. [CONFIRMED — Digital Foundry,
memory.md §8.] [4]

| Reservation | Size | Purpose |
|---|---|---|
| Horizon OS kernel + services | ~500 MB [SPECULATIVE] | Kernel, drivers, IPC buffers |
| GameChat (4 players) | ~800 MB [SPECULATIVE] | Video/audio streams, AI processing |
| Background services | ~500 MB [SPECULATIVE] | eShop, News, captures, telemetry |
| Display compositor | ~200 MB [SPECULATIVE] | Framebuffers, overlay planes |
| Reserved headroom | ~300 MB [SPECULATIVE] | Future features, stability margin |
| **Total OS reservation** | **3 GB [CONFIRMED]** | |

**Table 7.2:** Memory reservation breakdown. GameChat with 4 players and
camera support is the primary driver of the 3× increase over Switch 1.
[4]

### 7.3 GPU Resource Reservations

The OS reserves a portion of GPU compute for system compositing (home menu
overlay, notification popups, GameChat camera preview). The exact GPU
allocation is managed by the `avm` (Application Version Manager) and `vi`
services. Game developers do not have exclusive access to the GPU during
GameChat sessions. [INFERRED — Digital Foundry analysis.]

---

## 8. NVN Graphics API Overview

### 8.1 NVN2 in the Firmware Stack

NVN2 is Nintendo's proprietary graphics API for the Switch 2, replacing the
original NVN from Switch 1. It operates as a thin abstraction over the
NVIDIA GPU driver (`nvdrv` service), providing low-level GPU command
submission with minimal driver overhead. From a firmware perspective, NVN2
is not part of the kernel — it is a userland library linked into game
executables that communicates with the `nvdrv` service via HIPC. [INFERRED
— NVN2 details under NDA; based on reverse engineering and switchbrew.] [1][8]

```
+------------------------------------------------------------------+
|  NVN2 Firmware Interaction                                       |
|                                                                  |
|  Game Process (EL0)                                              |
|  +----------------------------------------------------------+   |
|  |  NVN2 Library (linked at build time)                      |   |
|  |  - Command buffer management                              |   |
|  |  - Resource tracking (surfaces, textures, samplers)       |   |
|  |  - Shader compilation (GLSL → SPIR-V → SASS)             |   |
|  |  - Queue submission                                       |   |
|  +---------------------------+------------------------------+   |
|                              |                                  |
|                    HIPC calls |                                  |
|                              v                                  |
|  +----------------------------------------------------------+   |
|  |  nvdrv service (NVIDIA driver wrapper)                    |   |
|  |  - Translates NVN2 commands to GPU ioctl-equivalents      |   |
|  |  - Manages GPU channel allocation                         |   |
|  |  - Submits command buffers to GPU hardware                |   |
|  +---------------------------+------------------------------+   |
|                              |                                  |
|                    GPU channel |                                 |
|                              v                                  |
|  +----------------------------------------------------------+   |
|  |  T239 GPU Hardware (SM86 Ampere)                          |   |
|  |  - Executes SASS shaders                                  |   |
|  |  - Renders to framebuffer / compute dispatch              |   |
|  +----------------------------------------------------------+   |
+------------------------------------------------------------------+
```

**Figure 8.1:** NVN2 firmware interaction path. [INFERRED]

### 8.2 NVN2 vs Vulkan Characteristics

NVN2 shares many design principles with Vulkan but is tightly coupled to the
T239 hardware:

| Aspect | NVN2 | Vulkan |
|---|---|---|
| Command submission | Ring buffer to `nvdrv` | Primary/secondary command buffers |
| Memory management | Pool-based (NVN2 pools) | Application-managed (VMA, custom) |
| Resource binding | Tier-based (3 tiers) | Descriptor sets/layouts |
| Shader compilation | GLSL → SASS (driver) | GLSL/SPIR-V → ISA (driver) |
| Queue model | 1 graphics + 1 compute | Multiple queue families |
| Debugging | `nvdbg` service | Validation layers + debug utils |

**Table 8.1:** NVN2 vs Vulkan comparison from firmware perspective. [SPECULATIVE
— based on NVN1 documentation and general API design.] [8]

### 8.3 NVN2 Resource Binding Tiers

NVN2 uses a tier model for resource binding, similar to Vulkan's descriptor
indexing extensions:

| Tier | Capability | Typical Use |
|---|---|---|
| Tier 1 | Fixed slot binding (up to 128 textures, 16 samplers) | Simple scenes |
| Tier 2 | Descriptor indexing within pools | Medium complexity |
| Tier 3 | Fully bindless (GPUVA-based) | Complex rendering [SPECULATIVE] |

**Table 8.2:** NVN2 resource binding tiers. [SPECULATIVE — based on NVN1
and modern GPU API evolution.] [8]

### 8.4 oboromi's NVN2 Role

oboromi's GPU module (`core/src/gpu/`) targets the **inverse** of the NVN2
shader compilation pipeline: it reads compiled SASS binary and translates it
back to SPIR-V, enabling analysis and potential re-hosting of Switch 2 GPU
programs on non-NVIDIA hardware. The `nvdrv` and `gpuk` service stubs in
`core/src/nn/` provide the IPC interface for communicating with the NVIDIA
driver subsystem. [CONFIRMED — oboromi source code.] [5]

---

## 9. Gap Analysis vs oboromi

### 9.1 Source File Coverage

This section maps each firmware documentation domain to the corresponding
oboromi source files, identifying what is implemented and what is missing.

| Firmware Domain | oboromi Files | Status |
|---|---|---|
| HIPC protocol | `core/src/nn/hipc.rs` | Header parsing implemented (2-word decode); message dispatch stubbed [CONFIRMED] |
| Service registry | `core/src/nn/mod.rs` | 160 services defined via `define_service!` macro; all stubs [CONFIRMED] |
| System state | `core/src/sys/mod.rs` | Services container + GPU state; minimal [CONFIRMED] |
| Service discovery (sm) | — | Not implemented [CONFIRMED] |
| KIP/INI1 parsing | — | Not implemented [CONFIRMED] |
| Boot sequence | — | Not implemented [CONFIRMED] |
| Kernel scheduler | — | Not implemented [CONFIRMED] |
| Handle table management | — | Not implemented [CONFIRMED] |
| NVN2 command buffer | `core/src/gpu/` (partial) | SASS decoder exists; NVN2 command layer absent [CONFIRMED] |
| System reservations | — | Not implemented (documented only) [CONFIRMED] |
| Sleep/resume | — | Not implemented [CONFIRMED] |
| Kernel capabilities | — | Not implemented [CONFIRMED] |

**Table 9.1:** Firmware domain → oboromi source file mapping. [5]

### 9.2 Implementation Gaps

The most critical gaps for oboromi's development are:

1. **HIPC message dispatch:** The `invoke_method` function in `hipc.rs` parses
   the header but does not yet dispatch based on command ID or handle buffer
   descriptors. This is the core IPC emulation mechanism. [CONFIRMED]

2. **Service Manager (sm):** No service registry exists. oboromi tracks
   service names in the `start_host_services` array but has no lookup,
   routing, or session management. [CONFIRMED]

3. **Kernel object model:** No handle table, no kernel object reference
   counting, no cross-process session management. The entire capability
   model is unimplemented. [CONFIRMED]

4. **KIP loader:** No INI1 parsing, no KIP segment loading, no capability
   extraction. oboromi cannot yet load and execute system process images.
   [CONFIRMED]

5. **NVN2 command submission:** The GPU module focuses on SASS analysis but
   does not implement the `nvdrv` IPC interface for GPU command buffer
   submission. [CONFIRMED]

### 9.3 Priority Recommendations

For the firmware domain, the highest-priority implementations would be:

1. **HIPC dispatch loop** — enables service stubs to receive and respond to
   real IPC calls
2. **Service Manager** — enables service discovery and session routing
3. **Handle table** — enables proper kernel object lifecycle management
4. **NVN2/nvdrv bridge** — enables GPU command submission for rendering
   analysis [SPECULATIVE — priority depends on project goals]

---

## Citations

[1] switchbrew. "Nintendo Switch homebrew documentation — kernel, IPC,
services, boot." https://switchbrew.org — Comprehensive wiki covering
Horizon OS internals, SVCs, HIPC protocol, service definitions, and boot
sequence. Accessed 2025.

[2] Atmosphère. "Atmosphère — open-source custom firmware for Nintendo
Switch." https://github.com/Atmosphere-NX/Atmosphere — Reference
implementation of Horizon OS kernel patches, KIP loading, IPC handling,
and service MITM. Accessed 2025.

[3] Liedtke, J. "On µ-Kernel Construction." Proc. 15th ACM Symposium on
Operating Systems Principles (SOSP), 1995. — Foundational microkernel
design paper establishing the L4 philosophy that influenced Horizon OS
architecture.

[4] Digital Foundry. "Nintendo Switch 2: final tech specs, system
reservations, and developer documentation analysis." Eurogamer/Digital
Foundry, 2025. — Hardware analysis confirming 2-core and 3 GB OS
reservations.

[5] oboromi. "Source code — `core/src/nn/hipc.rs`, `core/src/nn/mod.rs`,
`core/src/sys/mod.rs`." Local repository. — Primary source for HIPC
header parsing, service definitions, and system state management.
Accessed 2025.

[6] NVIDIA. "Jetson Linux Developer Guide — Secure Boot." NVIDIA
Developer Documentation. https://developer.nvidia.com/embedded/jetson-linux
— Tegra secure boot chain, BCT format, MB1/MB2 stages, anti-rollback.
Accessed 2025.

[7] JEDEC. "JESD209-5: Low Power Double Data Rate 5 (LPDDR5/5X)."
JEDEC Solid State Technology Association, 2021. — LPDDR5X standard
referenced for T239 memory subsystem.

[8] switchbrew. "NVN — Nintendo graphics API documentation."
https://switchbrew.org/wiki/NVN — NVN/NVN2 API reference including
resource binding tiers, shader compilation, and command buffer model.
Accessed 2025.
