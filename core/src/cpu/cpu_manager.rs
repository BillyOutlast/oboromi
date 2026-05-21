use log::info;

use crate::cpu::UnicornCPU;
use crate::mmio::gic::{GicDistributor, GicV3};
use crate::mmio::MmioDevice;
use crate::security::bootrom::{BootRom, BootResult, BootError};
use crate::security::efuse::EfuseArray;
use std::cell::RefCell;
use std::pin::Pin;
use std::rc::Rc;

pub const CORE_COUNT: usize = 8;

#[cfg(not(target_pointer_width = "64"))]
compile_error!("oboromi requires a 64-bit architecture to emulate 12GB of RAM.");
// 12GB Memory
pub const MEMORY_SIZE: u64 = 12 * 1024 * 1024 * 1024; 
pub const MEMORY_BASE: u64 = 0x0;

/// GICv3 MMIO base address (within the MMIO region)
pub const GIC_BASE: u64 = 0x10000000;
/// GICv3 MMIO region size (4MB)
pub const GIC_SIZE: u64 = 0x400000;

pub struct CpuManager {
    pub cores: Vec<UnicornCPU>,
    // Pin prevents reallocation from invalidating pointers
    #[allow(dead_code)]
    pub shared_memory: Pin<Box<[u8]>>,
    /// Shared GIC distributor state for all cores
    gic_dist: Option<Rc<RefCell<GicDistributor>>>,
}

impl CpuManager {
    /// Create a new CpuManager with the default (12GB) memory size.
    pub fn new() -> Self {
        Self::new_with_size(MEMORY_SIZE)
    }

    /// Create a new CpuManager with a custom memory size.
    ///
    /// Use this for tests that don't need 12GB (e.g., `256 * 1024 * 1024` for
    /// 256MB is sufficient for most functional tests). The minimum is larger
    /// than `MMIO_BASE + MMIO_SIZE` (~272MB) so stack can be placed above MMIO.
    pub fn new_with_size(memory_size: u64) -> Self {
        // Allocate zeroed memory
        // note: on modern OSs, this is lazily allocated (virtual memory)
        // and won't consume physical RAM until written to.
        let shared_memory = Pin::new(vec![0u8; memory_size as usize].into_boxed_slice());
        let memory_ptr = shared_memory.as_ptr() as *mut u8;

        let mut cores = Vec::with_capacity(CORE_COUNT);

        for i in 0..CORE_COUNT {
            // Create CPU core sharing the same memory pointer
            // Safety: The memory is owned by CpuManager and pinned in place
            // and UnicornCPU will use it for the lifetime of CpuManager.
            let cpu = unsafe { UnicornCPU::new_with_shared_mem(i as u32, memory_ptr, memory_size) };
            
            if let Some(cpu) = cpu {
                cores.push(cpu);
            } else {
                panic!("Failed to create Core {}", i);
            }
        }

        Self {
            cores,
            shared_memory,
            gic_dist: None,
        }
    }

    pub fn run_all(&self) {
        // for now, just step all cores sequentially (round-robin)
        // in the future, this would be threaded
        for (_i, core) in self.cores.iter().enumerate() {
            // just run one step for testing
            core.step();
        }
    }

    pub fn get_core(&self, id: usize) -> Option<&UnicornCPU> {
        self.cores.get(id)
    }

    /// Get a mutable reference to a specific core by ID
    pub fn get_core_mut(&mut self, id: usize) -> Option<&mut UnicornCPU> {
        self.cores.get_mut(id)
    }

    /// Register an MMIO device on all cores' buses.
    ///
    /// `factory` is called once per core to produce an independent device instance.
    /// Each call must return a fresh device — shared state between cores is the
    /// caller's responsibility.
    pub fn register_mmio_device<D: MmioDevice + 'static>(
        &mut self,
        name: &str,
        base: u64,
        size: u64,
        factory: impl Fn() -> D,
    ) {
        for (i, core) in self.cores.iter_mut().enumerate() {
            let device = factory();
            core.mmio_bus_mut().register_device(name, base, size, device);
            info!(
                "CpuManager: registered MMIO device '{}' at {:#x}..{:#x} on core {}",
                name, base, base + size, i
            );
        }
    }

    /// Get a read-only handle to a specific core's MMIO bus for inspection.
    ///
    /// Returns a `Ref<MmioBus>` guard — deref it to call read-only methods
    /// like `registered_devices()` or `find_device()`.
    ///
    /// Returns `None` if `core_id` is out of range.
    pub fn mmio_bus(&self, core_id: usize) -> Option<std::cell::Ref<'_, crate::mmio::MmioBus>> {
        if core_id >= self.cores.len() {
            return None;
        }
        Some(self.cores[core_id].mmio_bus_ref())
    }

    /// Register the GICv3 interrupt controller on all cores.
    ///
    /// Creates a shared distributor and gives each core its own GicV3 device
    /// instance backed by the same distributor state.
    ///
    /// Returns a clone of the shared distributor `Rc` for external
    /// manipulation (e.g., triggering interrupts in tests).
    pub fn register_gic(&mut self) -> Rc<RefCell<GicDistributor>> {
        let dist = Rc::new(RefCell::new(GicDistributor::new(self.cores.len())));
        self.gic_dist = Some(dist.clone());

        let core_count = self.cores.len();
        for (i, core) in self.cores.iter_mut().enumerate() {
            let device = GicV3::new_with_shared_dist(core_count, dist.clone());
            core.mmio_bus_mut().register_device("gicv3", GIC_BASE, GIC_SIZE, device);
            // Wire the distributor reference into the core so deliver_irq() can peek
            core.gic_dist = Some(dist.clone());
            info!(
                "CpuManager: registered GICv3 MMIO device at {:#x}..{:#x} on core {}",
                GIC_BASE, GIC_BASE + GIC_SIZE, i
            );
        }

        dist
    }

    /// Get a reference to the shared GIC distributor, if registered.
    pub fn gic(&self) -> Option<&Rc<RefCell<GicDistributor>>> {
        self.gic_dist.as_ref()
    }

    /// Convenience method: run the BootROM on core 0.
    ///
    /// Creates a `BootRom` from `efuse` and dispatches `boot()` on core 0.
    /// The caller is responsible for registering an eFuse MMIO device on the
    /// bus (via `register_mmio_device`) if the firmware itself accesses eFuse
    /// registers at `EFUSE_BASE`. For security-only tests (error-path
    /// validation), bus registration is optional — `BootRom` only needs
    /// the `EfuseArray` for key material, not for MMIO access.
    ///
    /// Returns `BootError::NoCpu` if core 0 is unavailable (should not happen
    /// with the default 8-core CpuManager).
    pub fn boot_rom(
        &mut self,
        efuse: &EfuseArray,
        firmware: &[u8],
    ) -> Result<BootResult, BootError> {
        let bootrom = BootRom::new(efuse);
        let core = self
            .get_core_mut(0)
            .ok_or(BootError::NoCpu)?;
        bootrom.boot(core, firmware)
    }
}
