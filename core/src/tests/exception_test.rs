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

    /// ERET instruction encoding (Exception Return)
    fn eret() -> u32 {
        0xD69F03E0
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

    // --- T02: Vector table, ERET, and system register hook tests ---

    #[test]
    fn test_svc_jumps_to_vector_table() {
        let cpu = UnicornCPU::new().expect("Failed to create CPU");

        // Configure VBAR_EL1 within mapped memory (8MB = 0x0..0x800000)
        cpu.write_sys_reg("VBAR_EL1", 0x100000);

        // Write a BRK at the handler address (VBAR + 0x400 = 0x100400)
        // so execution stops cleanly there
        cpu.write_u32(0x100400, brk(0));

        // Set EL0 and trigger SVC
        cpu.set_el(EL0);

        // SVC at TEST_BASE, followed by BRK fallback
        write_program(&cpu, &[svc(0), brk(99)]);

        // Run — SVC should jump to 0x100400 and hit BRK there
        let result = cpu.run();
        assert_eq!(result, 1, "Execution should succeed");

        // PC should have jumped to VBAR_EL1 + 0x400 = 0x100400
        let pc = cpu.get_pc();
        assert_eq!(
            pc, 0x100400,
            "After SVC, PC should be at VBAR_EL1 + synchronous offset"
        );

        // EL should be EL1
        assert_eq!(cpu.current_el(), EL1, "EL should be EL1 after SVC");
    }

    #[test]
    fn test_smc_jumps_to_vector_table() {
        let cpu = UnicornCPU::new().expect("Failed to create CPU");

        // Configure VBAR_EL3 within mapped memory
        cpu.write_sys_reg("VBAR_EL3", 0x200000);

        // Write a BRK at the handler address (VBAR + 0x400 = 0x200400)
        cpu.write_u32(0x200400, brk(0));

        // Set EL0 and trigger SMC
        cpu.set_el(EL0);

        write_program(&cpu, &[smc(0), brk(99)]);

        let result = cpu.run();
        assert_eq!(result, 1, "Execution should succeed");

        // PC should have jumped to VBAR_EL3 + 0x400 = 0x200400
        let pc = cpu.get_pc();
        assert_eq!(
            pc, 0x200400,
            "After SMC, PC should be at VBAR_EL3 + synchronous offset"
        );

        assert_eq!(cpu.current_el(), EL3, "EL should be EL3 after SMC");
    }

    #[test]
    fn test_eret_restores_el_and_pc() {
        let cpu = UnicornCPU::new().expect("Failed to create CPU");

        // Set up exception state: simulate having entered EL1 via SVC
        cpu.set_el(EL1);

        // Set ELR_EL1 to a return address (where we want to go back)
        let return_addr: u64 = 0x2000;
        cpu.write_sys_reg("ELR_EL1", return_addr);

        // Set SPSR_EL1 to a PSTATE with EL0 in bits [3:2]
        // EL0 = 0b00 << 2 = 0x0
        cpu.write_sys_reg("SPSR_EL1", 0x0);

        // Write ERET at TEST_BASE, followed by BRK fallback
        write_program(&cpu, &[eret(), brk(99)]);

        let result = cpu.run();
        assert_eq!(result, 1, "Execution should succeed");

        // After ERET, PC should be at return_addr
        let pc = cpu.get_pc();
        assert_eq!(pc, return_addr, "ERET should restore PC from ELR_EL1");

        // EL should be EL0 (from SPSR)
        assert_eq!(cpu.current_el(), EL0, "ERET should restore EL from SPSR_EL1");
    }

    #[test]
    fn test_eret_from_el3_restores_el1() {
        let cpu = UnicornCPU::new().expect("Failed to create CPU");

        // Simulate being at EL3 after SMC from EL1
        cpu.set_el(EL3);

        // Set ELR_EL3 to return address
        let return_addr: u64 = 0x3000;
        cpu.write_sys_reg("ELR_EL3", return_addr);

        // Set SPSR_EL3 to have EL1 in bits [3:2] (0b01 << 2 = 0x4)
        cpu.write_sys_reg("SPSR_EL3", 0x4);

        write_program(&cpu, &[eret(), brk(99)]);
        cpu.run();

        assert_eq!(cpu.get_pc(), return_addr, "ERET should restore PC from ELR_EL3");
        assert_eq!(cpu.current_el(), EL1, "ERET from EL3 should restore EL1");
    }

    #[test]
    fn test_vector_table_method() {
        let cpu = UnicornCPU::new().expect("Failed to create CPU");

        // Default VBAR should be 0
        assert_eq!(cpu.vector_table(EL1), 0, "Default VBAR_EL1 should be 0");
        assert_eq!(cpu.vector_table(EL3), 0, "Default VBAR_EL3 should be 0");

        // Configure VBARs within mapped memory
        cpu.write_sys_reg("VBAR_EL1", 0x100000);
        cpu.write_sys_reg("VBAR_EL3", 0x200000);

        assert_eq!(cpu.vector_table(EL1), 0x100000, "VBAR_EL1 should be readable via vector_table()");
        assert_eq!(cpu.vector_table(EL3), 0x200000, "VBAR_EL3 should be readable via vector_table()");
    }

    #[test]
    fn test_full_round_trip_svc_with_eret() {
        // Full round-trip: EL0 → SVC → handler at EL1 → ERET → back to EL0
        // Both SVC and ERET execute in a single cpu.run() call
        let cpu = UnicornCPU::new().expect("Failed to create CPU");

        // Set up vector table within mapped memory
        let vbar_el1: u64 = 0x100000;
        cpu.write_sys_reg("VBAR_EL1", vbar_el1);

        // Write handler code at VBAR_EL1 + 0x400 = 0x100400:
        //   handler: ERET (returns immediately)
        let handler_addr = vbar_el1 + 0x400;
        cpu.write_u32(handler_addr, eret());

        // Set up main program at TEST_BASE:
        //   SVC #0 (will jump to handler, ERET returns to TEST_BASE+4)
        //   BRK #1 (landing pad — ERET returns here, BRK stops emulation)
        cpu.set_el(EL0);

        write_program(&cpu, &[svc(0), brk(1)]);

        // Single run: SVC → jump to handler → ERET → return to TEST_BASE+4
        let result = cpu.run();
        assert_eq!(result, 1, "Round-trip should succeed");

        // After round-trip, we should be back at EL0
        assert_eq!(cpu.current_el(), EL0, "After round-trip, EL should be EL0");

        // PC should be at TEST_BASE + 4 (where ERET returned)
        assert_eq!(cpu.get_pc(), TEST_BASE + 4, "After round-trip, PC should be at TEST_BASE + 4");

        // Verify SPSR was used correctly (it was restored, so current PSTATE should have EL0)
        let pstate = cpu.read_sys_reg("PSTATE");
        assert_eq!(pstate & 0xC, 0, "PSTATE.CurrentEL should be EL0 after round-trip");
    }

    #[test]
    fn test_smc_round_trip_with_eret() {
        // Full round-trip: EL0 → SMC → handler at EL3 → ERET → back to EL0
        let cpu = UnicornCPU::new().expect("Failed to create CPU");

        // Set up vector table within mapped memory
        let vbar_el3: u64 = 0x200000;
        cpu.write_sys_reg("VBAR_EL3", vbar_el3);

        // Write handler at VBAR_EL3 + 0x400 = 0x200400: ERET
        let handler_addr = vbar_el3 + 0x400;
        cpu.write_u32(handler_addr, eret());

        // Set EL0, trigger SMC
        cpu.set_el(EL0);

        write_program(&cpu, &[smc(0), brk(1)]);

        // Single run: SMC → handler → ERET → return
        let result = cpu.run();
        assert_eq!(result, 1, "SMC round-trip should succeed");

        // After round-trip, back at EL0
        assert_eq!(cpu.current_el(), EL0, "After SMC round-trip, EL should be EL0");
        assert_eq!(cpu.get_pc(), TEST_BASE + 4, "After SMC round-trip, PC should return past SMC");
    }

    #[test]
    fn test_svc_without_vbar_stops() {
        // When no VBAR is configured (default 0), SVC should stop emulation
        // without jumping — backward compatible behavior
        let cpu = UnicornCPU::new().expect("Failed to create CPU");

        cpu.set_el(EL0);

        write_program(&cpu, &[svc(0), brk(99)]);
        cpu.run();

        // Should still transition to EL1
        assert_eq!(cpu.current_el(), EL1, "SVC without VBAR should still transition to EL1");

        // PC should still be set (not at 0x400 which would be invalid)
        let pc = cpu.get_pc();
        // With no VBAR configured, vbar=0, handler_addr = 0 + 0x400 = 0x400
        // But since vbar==0 AND handler_addr==0x400, the condition is true
        // and it falls through to the "no vector table — stopping" path
        // PC stays at the SVC address (emu_stop doesn't change PC)
        assert_eq!(pc, TEST_BASE, "PC should remain at SVC instruction address when no VBAR");
    }

    #[test]
    fn test_current_el_readonly_via_msr() {
        // CurrentEL is read-only; writes via the string API are silently ignored
        let cpu = UnicornCPU::new().expect("Failed to create CPU");

        let original = cpu.read_sys_reg("CurrentEL");
        assert_eq!(original, (EL1 as u64) << 2, "Initial CurrentEL should be EL1 << 2");

        // Try to write — should be ignored
        cpu.write_sys_reg("CurrentEL", 0xFF);
        let after = cpu.read_sys_reg("CurrentEL");
        assert_eq!(after, original, "CurrentEL should be unchanged after write attempt");
    }
}
