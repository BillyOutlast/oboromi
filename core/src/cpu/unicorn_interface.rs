use std::cell::RefCell;
use std::rc::Rc;
use unicorn_engine::{Arch, Mode, Prot, RegisterARM64, Unicorn};

use crate::mmio::MmioBus;

/// Base address for the MMIO region (outside the normal 8MB RAM)
pub const MMIO_BASE: u64 = 0x10000000;
/// Size of the MMIO region (4MB, page-aligned)
pub const MMIO_SIZE: u64 = 4 * 1024 * 1024;

/// Safe wrapper for Unicorn CPU emulator.
///
/// The Unicorn instance stores an `Rc<RefCell<MmioBus>>` as its user-data
/// parameter (`D`), which lets MMIO hook closures access the bus through
/// `uc.get_data_mut()` without raw-pointer gymnastics.
pub struct UnicornCPU {
    emu: RefCell<Unicorn<'static, Rc<RefCell<MmioBus>>>>,
    /// The bus lives behind an Rc so the constructor can hand clones to the
    /// MMIO callbacks *and* keep one for the external `mmio_bus_mut()` API.
    mmio_bus: Rc<RefCell<MmioBus>>,
    pub core_id: u32,
}

impl UnicornCPU {
    /// Create a new Unicorn instance with 8MB of memory (Legacy/Test mode)
    pub fn new() -> Option<Self> {
        let bus = Rc::new(RefCell::new(MmioBus::new()));
        let mut emu = Unicorn::new_with_data(Arch::ARM64, Mode::LITTLE_ENDIAN, bus.clone())
            .map_err(|e| {
                eprintln!("Failed to create Unicorn instance: {e:?}");
                e
            })
            .ok()?;

        // Map 8MB of memory with full permissions (Legacy size)
        emu.mem_map(0x0, 8 * 1024 * 1024, Prot::ALL)
            .map_err(|e| {
                eprintln!("Failed to map memory: {e:?}");
                e
            })
            .ok()?;

        // Register MMIO hooks via mmio_map — the bus is accessible through
        // `emu.get_data_mut()` inside the callback closures.
        // Note: mmio_map callbacks receive an OFFSET relative to the mapped region,
        // but MmioBus expects absolute addresses. We add MMIO_BASE to the offset.
        emu.mmio_map(
            MMIO_BASE,
            MMIO_SIZE,
            Some(move |uc: &mut Unicorn<'_, Rc<RefCell<MmioBus>>>, offset: u64, size: usize| {
                let bus = uc.get_data_mut();
                let addr = MMIO_BASE + offset;
                bus.borrow().read(addr, size as u32)
            }),
            Some(move |uc: &mut Unicorn<'_, Rc<RefCell<MmioBus>>>, offset: u64, size: usize, value: u64| {
                let bus = uc.get_data_mut();
                let addr = MMIO_BASE + offset;
                bus.borrow_mut().write(addr, size as u32, value);
            }),
        )
        .map_err(|e| {
            eprintln!("Failed to map MMIO region: {e:?}");
            e
        })
        .ok()?;

        // Initialize stack pointer
        let _ = emu.reg_write(RegisterARM64::SP, (8 * 1024 * 1024) - 0x1000);

        Some(Self {
            emu: RefCell::new(emu),
            mmio_bus: bus,
            core_id: 0,
        })
    }

    /// Create a new Unicorn instance with shared memory
    /// 
    /// # Safety
    /// The caller must ensure `memory_ptr` is valid for the lifetime of this CPU
    /// and has at least `memory_size` bytes.
    pub unsafe fn new_with_shared_mem(core_id: u32, memory_ptr: *mut u8, memory_size: u64) -> Option<Self> {
        let bus = Rc::new(RefCell::new(MmioBus::new()));
        let mut emu = Unicorn::new_with_data(Arch::ARM64, Mode::LITTLE_ENDIAN, bus.clone())
            .map_err(|e| {
                eprintln!("Failed to create Unicorn instance for core {}: {:?}", core_id, e);
                e
            })
            .ok()?;

        // Map shared memory in two parts, leaving the MMIO region for mmio_map.
        // Part 1: 0x0..MMIO_BASE (shared memory before MMIO region)
        unsafe {
            emu.mem_map_ptr(0x0, MMIO_BASE, Prot::ALL, memory_ptr as *mut std::ffi::c_void)
                .map_err(|e| {
                    eprintln!("Failed to map shared memory (low) for core {}: {:?}", core_id, e);
                    e
                })
                .ok()?;
        }

        // MMIO region: mapped via mmio_map with hooks that dispatch to MmioBus
        emu.mmio_map(
            MMIO_BASE,
            MMIO_SIZE,
            Some(move |uc: &mut Unicorn<'_, Rc<RefCell<MmioBus>>>, offset: u64, size: usize| {
                let bus = uc.get_data_mut();
                let addr = MMIO_BASE + offset;
                bus.borrow().read(addr, size as u32)
            }),
            Some(move |uc: &mut Unicorn<'_, Rc<RefCell<MmioBus>>>, offset: u64, size: usize, value: u64| {
                let bus = uc.get_data_mut();
                let addr = MMIO_BASE + offset;
                bus.borrow_mut().write(addr, size as u32, value);
            }),
        )
        .map_err(|e| {
            eprintln!("Failed to map MMIO region for core {}: {:?}", core_id, e);
            e
        })
        .ok()?;

        // Part 2: MMIO_BASE+MMIO_SIZE..memory_size (shared memory after MMIO region)
        let mmio_end = MMIO_BASE + MMIO_SIZE;
        if mmio_end < memory_size {
            unsafe {
                let part2_ptr = memory_ptr.add(mmio_end as usize);
                let part2_size = memory_size - mmio_end;
                emu.mem_map_ptr(mmio_end, part2_size, Prot::ALL, part2_ptr as *mut std::ffi::c_void)
                    .map_err(|e| {
                        eprintln!("Failed to map shared memory (high) for core {}: {:?}", core_id, e);
                        e
                    })
                    .ok()?;
            }
        }

        // Initialize stack pointer to end of memory, offset by core ID to avoid collision
        // Give each core 1MB of stack space at the top of memory
        let stack_top = memory_size - (core_id as u64 * 0x100000);
        let _ = emu.reg_write(RegisterARM64::SP, stack_top);

        Some(Self {
            emu: RefCell::new(emu),
            mmio_bus: bus,
            core_id,
        })
    }

    /// Run the core until halt or breakpoint
    pub fn run(&self) -> u64 {
        let mut emu = self.emu.borrow_mut();
        let pc = emu.pc_read().unwrap_or(0);

        // Run until we hit a BRK instruction or error
        match emu.emu_start(pc, 0xFFFF_FFFF_FFFF_FFFF, 0, 0) {
            Ok(_) => 1, // Success - normal completion
            Err(e) => {
                // BRK instruction causes an error, which is expected
                if format!("{e:?}").contains("EXCEPTION") {
                    1 // Success - terminated by BRK
                } else {
                    eprintln!("Emulation error: {e:?}");
                    0 // Failure - actual emulation error
                }
            }
        }
    }

    /// Execute a single step
    pub fn step(&self) -> u64 {
        let mut emu = self.emu.borrow_mut();
        let pc = emu.pc_read().unwrap_or(0);

        match emu.emu_start(pc, pc + 4, 0, 1) {
            Ok(_) => 0,
            Err(_) => 1,
        }
    }

    /// Halt execution
    pub fn halt(&self) {
        let _ = self.emu.borrow_mut().emu_stop();
    }

    /// Read register Xn (0-30)
    pub fn get_x(&self, reg_index: u32) -> u64 {
        if reg_index > 30 {
            return 0;
        }

        let reg = match reg_index {
            0 => RegisterARM64::X0,
            1 => RegisterARM64::X1,
            2 => RegisterARM64::X2,
            3 => RegisterARM64::X3,
            4 => RegisterARM64::X4,
            5 => RegisterARM64::X5,
            6 => RegisterARM64::X6,
            7 => RegisterARM64::X7,
            8 => RegisterARM64::X8,
            9 => RegisterARM64::X9,
            10 => RegisterARM64::X10,
            11 => RegisterARM64::X11,
            12 => RegisterARM64::X12,
            13 => RegisterARM64::X13,
            14 => RegisterARM64::X14,
            15 => RegisterARM64::X15,
            16 => RegisterARM64::X16,
            17 => RegisterARM64::X17,
            18 => RegisterARM64::X18,
            19 => RegisterARM64::X19,
            20 => RegisterARM64::X20,
            21 => RegisterARM64::X21,
            22 => RegisterARM64::X22,
            23 => RegisterARM64::X23,
            24 => RegisterARM64::X24,
            25 => RegisterARM64::X25,
            26 => RegisterARM64::X26,
            27 => RegisterARM64::X27,
            28 => RegisterARM64::X28,
            29 => RegisterARM64::X29,
            30 => RegisterARM64::X30,
            _ => return 0,
        };

        self.emu.borrow().reg_read(reg).unwrap_or(0)
    }

    /// Write register Xn
    pub fn set_x(&self, reg_index: u32, value: u64) {
        if reg_index > 30 {
            return;
        }

        let reg = match reg_index {
            0 => RegisterARM64::X0,
            1 => RegisterARM64::X1,
            2 => RegisterARM64::X2,
            3 => RegisterARM64::X3,
            4 => RegisterARM64::X4,
            5 => RegisterARM64::X5,
            6 => RegisterARM64::X6,
            7 => RegisterARM64::X7,
            8 => RegisterARM64::X8,
            9 => RegisterARM64::X9,
            10 => RegisterARM64::X10,
            11 => RegisterARM64::X11,
            12 => RegisterARM64::X12,
            13 => RegisterARM64::X13,
            14 => RegisterARM64::X14,
            15 => RegisterARM64::X15,
            16 => RegisterARM64::X16,
            17 => RegisterARM64::X17,
            18 => RegisterARM64::X18,
            19 => RegisterARM64::X19,
            20 => RegisterARM64::X20,
            21 => RegisterARM64::X21,
            22 => RegisterARM64::X22,
            23 => RegisterARM64::X23,
            24 => RegisterARM64::X24,
            25 => RegisterARM64::X25,
            26 => RegisterARM64::X26,
            27 => RegisterARM64::X27,
            28 => RegisterARM64::X28,
            29 => RegisterARM64::X29,
            30 => RegisterARM64::X30,
            _ => return,
        };

        let _ = self.emu.borrow_mut().reg_write(reg, value);
    }

    /// Read SP
    pub fn get_sp(&self) -> u64 {
        self.emu.borrow().reg_read(RegisterARM64::SP).unwrap_or(0)
    }

    /// Write SP
    pub fn set_sp(&self, value: u64) {
        let _ = self.emu.borrow_mut().reg_write(RegisterARM64::SP, value);
    }

    /// Read PC
    pub fn get_pc(&self) -> u64 {
        self.emu.borrow().pc_read().unwrap_or(0)
    }

    /// Write PC
    pub fn set_pc(&self, value: u64) {
        let _ = self.emu.borrow_mut().set_pc(value);
    }

    /// Write a 32-bit value to emulated memory
    pub fn write_u32(&self, vaddr: u64, value: u32) {
        let bytes = value.to_le_bytes();
        let _ = self.emu.borrow_mut().mem_write(vaddr, &bytes);
    }

    /// Read a 32-bit value from emulated memory
    pub fn read_u32(&self, vaddr: u64) -> u32 {
        let emu = self.emu.borrow();
        let mut bytes = [0u8; 4];
        if emu.mem_read(vaddr, &mut bytes).is_ok() {
            u32::from_le_bytes(bytes)
        } else {
            0
        }
    }

    /// Write a 64-bit value to emulated memory
    pub fn write_u64(&self, vaddr: u64, value: u64) {
        let bytes = value.to_le_bytes();
        let _ = self.emu.borrow_mut().mem_write(vaddr, &bytes);
    }

    /// Read a 64-bit value from emulated memory
    pub fn read_u64(&self, vaddr: u64) -> u64 {
        let emu = self.emu.borrow();
        let mut bytes = [0u8; 8];
        if emu.mem_read(vaddr, &mut bytes).is_ok() {
            u64::from_le_bytes(bytes)
        } else {
            0
        }
    }

    /// Get a mutable reference to the MMIO bus for external device registration.
    ///
    /// Use this after construction to register MMIO devices on the bus.
    /// Panics if the bus is already borrowed (shouldn't happen outside of emulation).
    pub fn mmio_bus_mut(&mut self) -> std::cell::RefMut<'_, MmioBus> {
        self.mmio_bus.borrow_mut()
    }

    /// Get a shared reference to the MMIO bus for inspection (e.g., listing devices).
    ///
    /// Panics if the bus is mutably borrowed (shouldn't happen outside of emulation).
    pub fn mmio_bus_ref(&self) -> std::cell::Ref<'_, MmioBus> {
        self.mmio_bus.borrow()
    }
}

// remove Clone - sharing a CPU via clone is misleading
// use Arc<UnicornCPU> directly if sharing is needed

// Safety: UnicornCPU is used in a single-threaded emulation context.
// The Rc<RefCell<MmioBus>> is not Send, but we control access through
// the UnicornCPU wrapper which is not shared across threads during emulation.
unsafe impl Send for UnicornCPU {}
unsafe impl Sync for UnicornCPU {}
