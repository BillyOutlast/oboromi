#[cfg(test)]
mod tests {
    use crate::cpu::cpu_manager::CpuManager;

    const TEST_MEM_SIZE: u64 = 512 * 1024 * 1024; // 512MB

    /// Encode `MOVZ Xd, #imm16, LSL #(hw*16)`.
    /// hw: 0=LSL#0, 1=LSL#16, 2=LSL#32, 3=LSL#48.
    fn movz(d: u32, imm16: u32, hw: u32) -> u32 {
        0xD2800000 | (hw << 21) | (imm16 << 5) | d
    }

    /// Encode `BRK #0` — halts emulation.
    fn brk() -> u32 {
        0xD4200000
    }

    /// Write a sequence of 32-bit instructions into CPU memory.
    fn write_code(cpu: &crate::cpu::UnicornCPU, addr: u64, insns: &[u32]) {
        for (i, insn) in insns.iter().enumerate() {
            cpu.write_u32(addr + (i as u64) * 4, *insn);
        }
    }

    #[test]
    fn test_multicore_initialization() {
        println!("Initializing 8-core CPU Manager with 512MB RAM...");
        let manager = CpuManager::new_with_size(TEST_MEM_SIZE);
        
        assert_eq!(manager.cores.len(), 8, "Should have 8 cores");
        assert_eq!(manager.shared_memory.len() as u64, TEST_MEM_SIZE, "Memory should be 512MB");
    }

    #[test]
    fn test_shared_memory_access() {
        println!("Testing shared memory between cores...");
        let manager = CpuManager::new_with_size(TEST_MEM_SIZE);
        
        let core0 = manager.get_core(0).expect("Core 0 missing");
        let core1 = manager.get_core(1).expect("Core 1 missing");

        // Write value using Core 0
        let test_addr = 0x1000;
        let test_val = 0xDEADBEEF;
        println!("Core 0 writing {:#x} to {:#x}", test_val, test_addr);
        core0.write_u32(test_addr, test_val);

        // Read value using Core 1
        let read_val = core1.read_u32(test_addr);
        println!("Core 1 read {:#x} from {:#x}", read_val, test_addr);

        assert_eq!(read_val, test_val, "Core 1 should see value written by Core 0");
    }

    #[test]
    fn test_per_core_execution() {
        println!("Verifying each core independently executes ARMv8 instructions...");
        let mut manager = CpuManager::new_with_size(TEST_MEM_SIZE);

        for core_id in 0..8 {
            let code_addr = 0x1000 + (core_id as u64) * 0x100;
            let expected = 0x100 + core_id as u64;

            // MOVZ X0, #expected, LSL#0
            // BRK #0
            let insns = [movz(0, expected as u32, 0), brk()];

            {
                let core = manager
                    .get_core_mut(core_id)
                    .expect("core should be accessible mutably");
                write_code(core, code_addr, &insns);
                core.set_pc(code_addr);
            }

            let core = manager.get_core(core_id).expect("core should be accessible");
            let result = core.run();
            assert!(
                result != 0,
                "Core {} should execute successfully (BRK-terminated)",
                core_id
            );

            let x0 = core.get_x(0);
            println!(
                "Core {}: X0 = {:#x} (expected {:#x}), PC = {:#x}",
                core_id,
                x0,
                expected,
                core.get_pc()
            );
            assert_eq!(
                x0, expected,
                "Core {} X0 should be {:#x} after MOVZ, got {:#x}",
                core_id, expected, x0
            );
        }

        println!("All 8 cores executed ARMv8 instructions independently");
    }
}
