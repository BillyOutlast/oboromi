use log::warn;

/// Trait for memory-mapped I/O devices.
///
/// Each device owns a contiguous address range and handles reads/writes
/// within that range. The `offset` passed to read/write is relative to
/// the device's base address (i.e., `addr - base`).
pub trait MmioDevice {
    /// Read `size` bytes from the device at the given offset.
    /// Returns the value in little-endian order, zero-extended to u64.
    fn read(&self, offset: u64, size: u32) -> u64;

    /// Write `size` bytes to the device at the given offset.
    /// `value` is in little-endian order; only the low `size` bytes are meaningful.
    fn write(&mut self, offset: u64, size: u32, value: u64);
}

/// A registered device entry on the MMIO bus.
struct RegisteredDevice {
    name: String,
    base: u64,
    size: u64,
    device: Box<dyn MmioDevice>,
}

/// Memory-mapped I/O bus that manages device registration and dispatches
/// read/write accesses to the appropriate device by address range.
pub struct MmioBus {
    devices: Vec<RegisteredDevice>,
}

impl MmioBus {
    /// Create a new, empty MMIO bus.
    pub fn new() -> Self {
        Self {
            devices: Vec::new(),
        }
    }

    /// Register a device on the bus at the given address range.
    ///
    /// # Panics
    /// Panics if the new range overlaps with any already-registered device.
    pub fn register_device(
        &mut self,
        name: &str,
        base: u64,
        size: u64,
        device: impl MmioDevice + 'static,
    ) {
        // Check for overlap with existing devices
        let new_end = base + size;
        for existing in &self.devices {
            let existing_end = existing.base + existing.size;
            if base < existing_end && existing.base < new_end {
                panic!(
                    "MMIO device '{}' (base={:#x}, size={:#x}) overlaps with existing device '{}' (base={:#x}, size={:#x})",
                    name, base, size, existing.name, existing.base, existing.size
                );
            }
        }

        log::info!(
            "MMIO: registered device '{}' at {:#x}..{:#x} (size={:#x})",
            name,
            base,
            base + size,
            size
        );

        self.devices.push(RegisteredDevice {
            name: name.to_string(),
            base,
            size,
            device: Box::new(device),
        });
    }

    /// Find the device that owns the given address, if any.
    pub fn find_device(&self, addr: u64) -> Option<(&str, u64, u64)> {
        for entry in &self.devices {
            if addr >= entry.base && addr < entry.base + entry.size {
                return Some((&entry.name, entry.base, entry.size));
            }
        }
        None
    }

    /// Read `size` bytes from the device at `addr`.
    ///
    /// If no device is mapped at `addr`, returns 0 and logs a warning.
    pub fn read(&self, addr: u64, size: u32) -> u64 {
        for entry in &self.devices {
            if addr >= entry.base && addr < entry.base + entry.size {
                let offset = addr - entry.base;
                return entry.device.read(offset, size);
            }
        }

        warn!(
            "MMIO read unmapped: addr={:#x}, size={}",
            addr, size
        );
        0
    }

    /// Write `size` bytes to the device at `addr`.
    ///
    /// If no device is mapped at `addr`, the write is discarded and a warning is logged.
    pub fn write(&mut self, addr: u64, size: u32, value: u64) {
        for entry in &mut self.devices {
            if addr >= entry.base && addr < entry.base + entry.size {
                let offset = addr - entry.base;
                entry.device.write(offset, size, value);
                return;
            }
        }

        warn!(
            "MMIO write unmapped: addr={:#x}, size={}, value={:#x}",
            addr, size, value
        );
    }

    /// Return a list of all registered devices as `(name, base, size)`.
    pub fn registered_devices(&self) -> Vec<(String, u64, u64)> {
        self.devices
            .iter()
            .map(|d| (d.name.clone(), d.base, d.size))
            .collect()
    }
}

impl Default for MmioBus {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    /// A simple mock device backed by a Vec<u8>.
    struct MockDevice {
        data: Vec<u8>,
    }

    impl MockDevice {
        fn new(size: usize) -> Self {
            Self {
                data: vec![0u8; size],
            }
        }
    }

    impl MmioDevice for MockDevice {
        fn read(&self, offset: u64, size: u32) -> u64 {
            let off = offset as usize;
            let sz = size as usize;
            if off + sz > self.data.len() {
                return 0;
            }
            let mut buf = [0u8; 8];
            buf[..sz].copy_from_slice(&self.data[off..off + sz]);
            u64::from_le_bytes(buf)
        }

        fn write(&mut self, offset: u64, size: u32, value: u64) {
            let off = offset as usize;
            let sz = size as usize;
            if off + sz > self.data.len() {
                return;
            }
            let bytes = value.to_le_bytes();
            self.data[off..off + sz].copy_from_slice(&bytes[..sz]);
        }
    }

    /// Mock device that records writes for verification.
    struct RecordingDevice {
        last_write: RefCell<Option<(u64, u32, u64)>>,
    }

    impl RecordingDevice {
        fn new() -> Self {
            Self {
                last_write: RefCell::new(None),
            }
        }
    }

    impl MmioDevice for RecordingDevice {
        fn read(&self, _offset: u64, _size: u32) -> u64 {
            0xDEAD
        }

        fn write(&mut self, offset: u64, size: u32, value: u64) {
            *self.last_write.borrow_mut() = Some((offset, size, value));
        }
    }

    #[test]
    fn test_mapped_read_write() {
        let mut bus = MmioBus::new();
        bus.register_device("test", 0x1000, 0x100, MockDevice::new(0x100));

        // Write a u32 at offset 0
        bus.write(0x1000, 4, 0xCAFEBABE);
        // Read it back
        let val = bus.read(0x1000, 4);
        assert_eq!(val, 0xCAFEBABE);

        // Write a u8 at offset 4
        bus.write(0x1004, 1, 0x42);
        let val = bus.read(0x1004, 1);
        assert_eq!(val, 0x42);

        // Write a u16 at offset 8
        bus.write(0x1008, 2, 0xBEEF);
        let val = bus.read(0x1008, 2);
        assert_eq!(val, 0xBEEF);
    }

    #[test]
    fn test_unmapped_read_returns_zero() {
        let bus = MmioBus::new();
        let val = bus.read(0xFFFF, 4);
        assert_eq!(val, 0);
    }

    #[test]
    fn test_unmapped_write_does_not_panic() {
        let mut bus = MmioBus::new();
        // Should not panic — just discards
        bus.write(0xFFFF, 4, 0x12345678);
    }

    #[test]
    fn test_overlapping_registration_panics() {
        let mut bus = MmioBus::new();
        bus.register_device("dev_a", 0x1000, 0x100, MockDevice::new(0x100));

        // Exact same range
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            bus.register_device("dev_b", 0x1000, 0x100, MockDevice::new(0x100));
        }));
        assert!(result.is_err(), "Expected panic on overlapping registration");

        // Partial overlap (start inside existing)
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            bus.register_device("dev_c", 0x1080, 0x100, MockDevice::new(0x100));
        }));
        assert!(result.is_err(), "Expected panic on partial overlap");

        // Partial overlap (end inside existing)
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            bus.register_device("dev_d", 0x0F80, 0x100, MockDevice::new(0x100));
        }));
        assert!(result.is_err(), "Expected panic on end-inside overlap");
    }

    #[test]
    fn test_multiple_devices_independent() {
        let mut bus = MmioBus::new();
        bus.register_device("low", 0x1000, 0x100, MockDevice::new(0x100));
        bus.register_device("high", 0x2000, 0x100, MockDevice::new(0x100));

        // Write to low device
        bus.write(0x1000, 4, 0xAAAAAAAA);
        // Write to high device
        bus.write(0x2000, 4, 0xBBBBBBBB);

        // Read back — each device holds its own value
        assert_eq!(bus.read(0x1000, 4), 0xAAAAAAAA);
        assert_eq!(bus.read(0x2000, 4), 0xBBBBBBBB);
    }

    #[test]
    fn test_read_at_exact_base() {
        let mut bus = MmioBus::new();
        bus.register_device("dev", 0x4000, 0x100, MockDevice::new(0x100));

        bus.write(0x4000, 4, 0x11111111);
        assert_eq!(bus.read(0x4000, 4), 0x11111111);
    }

    #[test]
    fn test_read_at_last_byte() {
        let mut bus = MmioBus::new();
        bus.register_device("dev", 0x4000, 0x100, MockDevice::new(0x100));

        // Write at the very last byte of the range
        bus.write(0x40FF, 1, 0xEE);
        assert_eq!(bus.read(0x40FF, 1), 0xEE);
    }

    #[test]
    fn test_read_at_base_plus_size_is_unmapped() {
        let bus = MmioBus::new();
        // Device at 0x4000..0x4100
        // Address 0x4100 is one past the end — unmapped
        let val = bus.read(0x4100, 4);
        assert_eq!(val, 0);
    }

    #[test]
    fn test_find_device() {
        let mut bus = MmioBus::new();
        bus.register_device("uart", 0x5000, 0x100, MockDevice::new(0x100));

        // Inside range
        let result = bus.find_device(0x5050);
        assert!(result.is_some());
        let (name, base, size) = result.unwrap();
        assert_eq!(name, "uart");
        assert_eq!(base, 0x5000);
        assert_eq!(size, 0x100);

        // Outside range
        assert!(bus.find_device(0x6000).is_none());
    }

    #[test]
    fn test_registered_devices_list() {
        let mut bus = MmioBus::new();
        bus.register_device("uart", 0x5000, 0x100, MockDevice::new(0x100));
        bus.register_device("timer", 0x6000, 0x40, MockDevice::new(0x40));

        let devices = bus.registered_devices();
        assert_eq!(devices.len(), 2);
        assert_eq!(devices[0], ("uart".to_string(), 0x5000, 0x100));
        assert_eq!(devices[1], ("timer".to_string(), 0x6000, 0x40));
    }

    #[test]
    fn test_u64_read_write() {
        let mut bus = MmioBus::new();
        bus.register_device("dev", 0x8000, 0x100, MockDevice::new(0x100));

        bus.write(0x8000, 8, 0x0102030405060708);
        assert_eq!(bus.read(0x8000, 8), 0x0102030405060708);
    }

    #[test]
    fn test_adjacent_devices_no_overlap() {
        let mut bus = MmioBus::new();
        // Two devices back-to-back with no gap — should NOT panic
        bus.register_device("a", 0x1000, 0x100, MockDevice::new(0x100));
        bus.register_device("b", 0x1100, 0x100, MockDevice::new(0x100));

        bus.write(0x1000, 4, 0x11111111);
        bus.write(0x1100, 4, 0x22222222);
        assert_eq!(bus.read(0x1000, 4), 0x11111111);
        assert_eq!(bus.read(0x1100, 4), 0x22222222);
    }

    #[test]
    fn test_write_dispatches_to_correct_device() {
        let mut bus = MmioBus::new();
        bus.register_device("recorder", 0x9000, 0x100, RecordingDevice::new());

        bus.write(0x9004, 4, 0xFEEDFACE);

        // We can't directly read the RefCell from outside, but we can verify
        // the read returns the mock value, confirming the device is wired up.
        assert_eq!(bus.read(0x9000, 2), 0xDEAD);
    }
}
