#[cfg(test)]
mod tests {
    use crate::cpu::UnicornCPU;
    use crate::cpu::exception::{EL0, EL1, EL3};

    /// ARM64 instruction helpers for exception tests
    fn svc(imm16: u16) -> u32 {
        0xD4000001 | ((imm16 as u32) << 5)
    }

    fn smc(imm16: u16) -> u32 {
        0xD4000003 | ((imm16 as u32) << 5)
    }

    fn brk(imm16: u16) -> u32 {
        0xD4200000 | ((imm16 as u32) << 5)
    }

    const TEST_BASE: u64 = 0x1000;

    /// Write a sequence of instructions starting at TEST_BASE and set PC there
    fn write_program(cpu: &UnicornCPU, instructions: &[u32]) {
        let mut addr = TEST_BASE;
        for &instr in instructions {
            cpu.write_u32(addr, instr);
            addr += 4;
        }
        cpu.set_pc(TEST_BASE);
    }

    #[test]
    fn test_initial_exception_level() {
        let cpu = UnicornCPU::new().expect("Failed to create CPU");

        // Unicorn starts at EL1 by default
        assert_eq!(cpu.current_el(), EL1, "Core 0 should start at EL1");
    }

    #[test]
    fn test_svc_transitions_el0_to_el1() {
        let cpu = UnicornCPU::new().expect("Failed to create CPU");

        // Set EL0, then SVC should transition back to EL1
        cpu.set_el(EL0);
        assert_eq!(cpu.current_el(), EL0);

        // Write SVC #0 followed by BRK as fallback
        write_program(&cpu, &[svc(0), brk(99)]);

        // Run — SVC should trigger the hook and stop emulation
        let result = cpu.run();
        assert_eq!(result, 1, "Execution should succeed (SVC stops emulation)");

        // Verify EL transition: EL0 → EL1
        assert_eq!(cpu.current_el(), EL1, "SVC should transition from EL0 to EL1");
    }

    #[test]
    fn test_smc_transitions_to_el3() {
        let cpu = UnicornCPU::new().expect("Failed to create CPU");

        // Set EL0, then SMC should jump to EL3
        cpu.set_el(EL0);

        write_program(&cpu, &[smc(0), brk(99)]);

        let result = cpu.run();
        assert_eq!(result, 1, "Execution should succeed (SMC stops emulation)");

        assert_eq!(cpu.current_el(), EL3, "SMC should transition to EL3");
    }

    #[test]
    fn test_smc_from_el1_to_el3() {
        let cpu = UnicornCPU::new().expect("Failed to create CPU");

        // Start at EL1 (default), SMC should transition to EL3
        write_program(&cpu, &[smc(1), brk(99)]);

        let result = cpu.run();
        assert_eq!(result, 1, "Execution should succeed");

        assert_eq!(cpu.current_el(), EL3, "SMC from EL1 should transition to EL3");
    }

    #[test]
    fn test_spsr_el1_saved_on_svc() {
        let cpu = UnicornCPU::new().expect("Failed to create CPU");

        // Set EL0 and a known PSTATE (EL0 bits = 0 in [3:2], DAIF = 0x3C0 in [9:6])
        cpu.set_el(EL0);
        cpu.write_sys_reg("PSTATE", 0x3C0); // EL0 + all DAIF bits masked

        write_program(&cpu, &[svc(0), brk(99)]);
        cpu.run();

        let spsr = cpu.read_sys_reg("SPSR_EL1");
        assert_ne!(spsr, 0, "SPSR_EL1 should be saved on SVC (non-zero PSTATE)");
        // The saved PSTATE should have EL0 in bits [3:2]
        assert_eq!(spsr & 0xC, 0, "Saved SPSR_EL1 should have EL0 (bits [3:2] = 0)");
    }

    #[test]
    fn test_spsr_el3_saved_on_smc() {
        let cpu = UnicornCPU::new().expect("Failed to create CPU");

        // Set known PSTATE at EL1
        cpu.write_sys_reg("PSTATE", 0x3C4); // EL1 + some DAIF

        write_program(&cpu, &[smc(0), brk(99)]);
        cpu.run();

        let spsr = cpu.read_sys_reg("SPSR_EL3");
        assert_ne!(spsr, 0, "SPSR_EL3 should be saved on SMC");
        // The saved PSTATE should have EL1 in bits [3:2]
        assert_eq!(spsr & 0xC, 0x4, "Saved SPSR_EL3 should have EL1 (bits [3:2] = 0b01)");
    }

    #[test]
    fn test_elr_el1_set_on_svc() {
        let cpu = UnicornCPU::new().expect("Failed to create CPU");

        cpu.set_el(EL0);

        // SVC at address TEST_BASE (0x1000)
        write_program(&cpu, &[svc(0), brk(99)]);
        cpu.run();

        let elr = cpu.read_sys_reg("ELR_EL1");
        assert_eq!(elr, TEST_BASE + 4, "ELR_EL1 should be PC+4 after SVC at {TEST_BASE:#x}");
    }

    #[test]
    fn test_elr_el3_set_on_smc() {
        let cpu = UnicornCPU::new().expect("Failed to create CPU");

        // SMC at address TEST_BASE (0x1000)
        write_program(&cpu, &[smc(0), brk(99)]);
        cpu.run();

        let elr = cpu.read_sys_reg("ELR_EL3");
        assert_eq!(elr, TEST_BASE + 4, "ELR_EL3 should be PC+4 after SMC at {TEST_BASE:#x}");
    }

    #[test]
    fn test_read_write_current_el() {
        let cpu = UnicornCPU::new().expect("Failed to create CPU");

        // Read initial EL
        assert_eq!(cpu.current_el(), EL1);

        // Set to EL0
        cpu.set_el(EL0);
        assert_eq!(cpu.current_el(), EL0);

        // Set to EL3
        cpu.set_el(EL3);
        assert_eq!(cpu.current_el(), EL3);

        // Verify PSTATE.CurrentEL was updated
        let pstate = cpu.read_sys_reg("PSTATE");
        assert_eq!((pstate >> 2) & 0x3, EL3 as u64, "PSTATE.CurrentEL should reflect EL3");
    }

    #[test]
    fn test_daif_register() {
        let cpu = UnicornCPU::new().expect("Failed to create CPU");

        // Write DAIF mask (all interrupts masked: D=1, A=1, I=1, F=1 → bits [9:6] = 0x3C0)
        cpu.write_sys_reg("DAIF", 0x3C0);
        let daif = cpu.read_sys_reg("DAIF");
        assert_eq!(daif, 0x3C0, "DAIF should store masked interrupt bits");

        // Write partial DAIF (only IRQ masked: I=1 → bit 7 = 0x80)
        cpu.write_sys_reg("DAIF", 0x80);
        let daif = cpu.read_sys_reg("DAIF");
        assert_eq!(daif, 0x80, "DAIF should accept partial mask");
    }

    #[test]
    fn test_vbar_registers() {
        let cpu = UnicornCPU::new().expect("Failed to create CPU");

        // Write VBAR_EL1 (exception vector base for EL1)
        cpu.write_sys_reg("VBAR_EL1", 0x8000_0000);
        let vbar1 = cpu.read_sys_reg("VBAR_EL1");
        assert_eq!(vbar1, 0x8000_0000, "VBAR_EL1 should be readable");

        // Write VBAR_EL3 (exception vector base for EL3)
        cpu.write_sys_reg("VBAR_EL3", 0x9000_0000);
        let vbar3 = cpu.read_sys_reg("VBAR_EL3");
        assert_eq!(vbar3, 0x9000_0000, "VBAR_EL3 should be readable");
    }

    #[test]
    fn test_spsr_el1_read_write() {
        let cpu = UnicornCPU::new().expect("Failed to create CPU");

        cpu.write_sys_reg("SPSR_EL1", 0xDEAD_BEEF_CAFE_BABE);
        let spsr = cpu.read_sys_reg("SPSR_EL1");
        assert_eq!(spsr, 0xDEAD_BEEF_CAFE_BABE, "SPSR_EL1 should be readable and writable");
    }

    #[test]
    fn test_spsr_el3_read_write() {
        let cpu = UnicornCPU::new().expect("Failed to create CPU");

        cpu.write_sys_reg("SPSR_EL3", 0x1234_5678_9ABC_DEF0);
        let spsr = cpu.read_sys_reg("SPSR_EL3");
        assert_eq!(spsr, 0x1234_5678_9ABC_DEF0, "SPSR_EL3 should be readable and writable");
    }

    #[test]
    fn test_elr_el1_read_write() {
        let cpu = UnicornCPU::new().expect("Failed to create CPU");

        cpu.write_sys_reg("ELR_EL1", 0x4000);
        let elr = cpu.read_sys_reg("ELR_EL1");
        assert_eq!(elr, 0x4000, "ELR_EL1 should be readable and writable through ExceptionModule");
    }

    #[test]
    fn test_elr_el3_read_write() {
        let cpu = UnicornCPU::new().expect("Failed to create CPU");

        cpu.write_sys_reg("ELR_EL3", 0x5000);
        let elr = cpu.read_sys_reg("ELR_EL3");
        assert_eq!(elr, 0x5000, "ELR_EL3 should be readable and writable through ExceptionModule");
    }

    #[test]
    fn test_svc_with_immediate() {
        let cpu = UnicornCPU::new().expect("Failed to create CPU");

        cpu.set_el(EL0);

        // SVC #42 — the immediate is passed through; verify EL transition happens
        write_program(&cpu, &[svc(42), brk(99)]);
        cpu.run();

        assert_eq!(cpu.current_el(), EL1, "SVC #42 should still transition EL0→EL1");
    }

    #[test]
    fn test_smc_with_immediate() {
        let cpu = UnicornCPU::new().expect("Failed to create CPU");

        // SMC #7
        write_program(&cpu, &[smc(7), brk(99)]);
        cpu.run();

        assert_eq!(cpu.current_el(), EL3, "SMC #7 should transition to EL3");
    }

    #[test]
    fn test_exception_level_preserves_across_cores() {
        let manager = crate::cpu::cpu_manager::CpuManager::new();

        // SVC on core 0
        {
            let core0 = manager.get_core(0).expect("Core 0");
            core0.set_el(EL0);
            write_program(core0, &[svc(0), brk(99)]);
            core0.run();
        }

        // Verify core 0 is at EL1
        {
            let core0 = manager.get_core(0).expect("Core 0");
            assert_eq!(core0.current_el(), EL1, "Core 0 should be at EL1 after SVC");
        }

        // Verify core 1 is still at EL1 (default, unaffected)
        {
            let core1 = manager.get_core(1).expect("Core 1");
            assert_eq!(core1.current_el(), EL1, "Core 1 should remain at EL1 (unaffected by core 0 SVC)");
        }
    }

    #[test]
    fn test_pstate_el_field_updated_on_transition() {
        let cpu = UnicornCPU::new().expect("Failed to create CPU");

        cpu.set_el(EL0);

        write_program(&cpu, &[svc(0), brk(99)]);
        cpu.run();

        // Read PSTATE directly and verify the EL field
        let pstate = cpu.read_sys_reg("PSTATE");
        let pstate_el = (pstate >> 2) & 0x3;
        assert_eq!(pstate_el, EL1 as u64, "PSTATE.CurrentEL should be EL1 after SVC");
    }
}
