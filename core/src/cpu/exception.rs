use std::cell::RefCell;
use std::rc::Rc;

use log::{info, warn};
use unicorn_engine::{RegisterARM64, Unicorn};

use crate::mmio::MmioBus;

/// ARM64 Exception Level constants
pub const EL0: u8 = 0; // User space
pub const EL1: u8 = 1; // Kernel / OS
pub const EL2: u8 = 2; // Hypervisor
pub const EL3: u8 = 3; // Secure Monitor / Firmware

/// SVC instruction encoding mask and match value
/// SVC #imm16 = 0xD4000001 | (imm16 << 5)
const SVC_MASK: u32 = 0xFFE0001F;
const SVC_MATCH: u32 = 0xD4000001;

/// SMC instruction encoding mask and match value
/// SMC #imm16 = 0xD4000003 | (imm16 << 5)
const SMC_MASK: u32 = 0xFFE0001F;
const SMC_MATCH: u32 = 0xD4000003;

/// Exception module for ARM64 exception level management.
///
/// Tracks per-core exception level state and provides software-emulated
/// access to banked system registers (SPSR_ELx, ELR_ELx) that Unicorn
/// does not expose directly.
pub struct ExceptionModule {
    /// Current exception level per core
    current_el: [u8; 8],
    /// SPSR_EL1 per core (software-emulated, not exposed by Unicorn)
    spsr_el1: [u64; 8],
    /// SPSR_EL3 per core (software-emulated, not exposed by Unicorn)
    spsr_el3: [u64; 8],
    /// DAIF interrupt mask bits per core (software-emulated)
    /// Bits: [9]=D (debug), [8]=A (SError), [7]=I (IRQ), [6]=F (FIQ)
    daif: [u64; 8],
    /// VBAR_EL1 per core (software-emulated exception vector base)
    vbar_el1: [u64; 8],
    /// VBAR_EL3 per core (software-emulated exception vector base)
    vbar_el3: [u64; 8],
}

impl ExceptionModule {
    /// Create a new ExceptionModule. All cores start at EL1 by default
    /// (Unicorn emulates at EL1).
    pub fn new() -> Self {
        Self {
            current_el: [EL1; 8],
            spsr_el1: [0; 8],
            spsr_el3: [0; 8],
            daif: [0; 8],
            vbar_el1: [0; 8],
            vbar_el3: [0; 8],
        }
    }

    /// Register SVC/SMC code hooks on a Unicorn instance.
    ///
    /// The hooks intercept SVC and SMC instructions and perform manual
    /// exception level transitions, since Unicorn does not natively
    /// support ARM64 exception level switching.
    pub fn register_hooks(
        emu: &mut Unicorn<'static, Rc<RefCell<MmioBus>>>,
        exc_state: Rc<RefCell<ExceptionModule>>,
        core_id: u32,
    ) {
        let exc = exc_state.clone();
        let cid = core_id as usize;
        let _ = emu.add_code_hook(0, u64::MAX, move |uc, addr, _size| {
            handle_exception_instruction(uc, addr, &exc, cid);
        });
    }

    /// Get the current exception level for a core.
    pub fn current_el(&self, core_id: usize) -> u8 {
        if core_id < 8 {
            self.current_el[core_id]
        } else {
            EL0
        }
    }

    /// Set the current exception level for a core.
    /// Updates both the software tracking and PSTATE.CurrentEL in the emulator.
    pub fn set_current_el(
        &mut self,
        core_id: usize,
        el: u8,
        emu: &mut Unicorn<'static, Rc<RefCell<MmioBus>>>,
    ) {
        if core_id >= 8 || el > 3 {
            return;
        }
        self.current_el[core_id] = el;

        // Update PSTATE.CurrentEL in the emulator
        let pstate = emu.reg_read(RegisterARM64::PSTATE).unwrap_or(0);
        let new_pstate = (pstate & !0xC) | ((el as u64) << 2);
        let _ = emu.reg_write(RegisterARM64::PSTATE, new_pstate);
    }

    // --- System register access through ExceptionModule ---

    /// Read a system register. Delegates to Unicorn for registers it exposes
    /// (ELR_ELx, PSTATE), uses software emulation for banked registers
    /// (SPSR_ELx, DAIF, VBAR_ELx, CurrentEL).
    pub fn read_sys_reg(
        &self,
        reg: &str,
        core_id: usize,
        emu: &Unicorn<'static, Rc<RefCell<MmioBus>>>,
    ) -> u64 {
        if core_id >= 8 {
            return 0;
        }
        match reg {
            "CurrentEL" => (self.current_el[core_id] as u64) << 2,
            "DAIF" => self.daif[core_id],
            "VBAR_EL1" => self.vbar_el1[core_id],
            "VBAR_EL3" => self.vbar_el3[core_id],
            "SPSR_EL1" => self.spsr_el1[core_id],
            "SPSR_EL3" => self.spsr_el3[core_id],
            "ELR_EL1" => emu.reg_read(RegisterARM64::ELR_EL1).unwrap_or(0),
            "ELR_EL3" => emu.reg_read(RegisterARM64::ELR_EL3).unwrap_or(0),
            "PSTATE" => emu.reg_read(RegisterARM64::PSTATE).unwrap_or(0),
            _ => {
                warn!("read_sys_reg: unknown register '{reg}'");
                0
            }
        }
    }

    /// Write a system register. Delegates to Unicorn for registers it exposes
    /// (ELR_ELx, PSTATE), uses software emulation for banked registers
    /// (SPSR_ELx, DAIF, VBAR_ELx).
    pub fn write_sys_reg(
        &mut self,
        reg: &str,
        core_id: usize,
        value: u64,
        emu: &mut Unicorn<'static, Rc<RefCell<MmioBus>>>,
    ) {
        if core_id >= 8 {
            return;
        }
        match reg {
            "DAIF" => {
                // Only store bits [9:6] (D, A, I, F)
                self.daif[core_id] = value & 0x3C0;
            }
            "VBAR_EL1" => {
                self.vbar_el1[core_id] = value;
            }
            "VBAR_EL3" => {
                self.vbar_el3[core_id] = value;
            }
            "SPSR_EL1" => {
                self.spsr_el1[core_id] = value;
            }
            "SPSR_EL3" => {
                self.spsr_el3[core_id] = value;
            }
            "ELR_EL1" => {
                let _ = emu.reg_write(RegisterARM64::ELR_EL1, value);
            }
            "ELR_EL3" => {
                let _ = emu.reg_write(RegisterARM64::ELR_EL3, value);
            }
            "PSTATE" => {
                let _ = emu.reg_write(RegisterARM64::PSTATE, value);
                // Sync the EL field from PSTATE back to software tracking
                self.current_el[core_id] = ((value >> 2) & 0x3) as u8;
            }
            _ => {
                warn!("write_sys_reg: unknown register '{reg}'");
            }
        }
    }
}

// --- Free functions with generic lifetime for hook callbacks ---

/// Check if an instruction at the given address is SVC or SMC, and if so,
/// perform the exception transition.
fn handle_exception_instruction(
    uc: &mut Unicorn<'_, Rc<RefCell<MmioBus>>>,
    addr: u64,
    exc_state: &Rc<RefCell<ExceptionModule>>,
    core_id: usize,
) {
    // Read the instruction at the current PC
    let mut instr_bytes = [0u8; 4];
    if uc.mem_read(addr, &mut instr_bytes).is_err() {
        return;
    }
    let instr = u32::from_le_bytes(instr_bytes);

    if (instr & SVC_MASK) == SVC_MATCH {
        let imm16 = (instr >> 5) & 0xFFFF;
        handle_svc(uc, addr, imm16, exc_state, core_id);
    } else if (instr & SMC_MASK) == SMC_MATCH {
        let imm16 = (instr >> 5) & 0xFFFF;
        handle_smc(uc, addr, imm16, exc_state, core_id);
    }
}

/// Handle SVC (Supervisor Call) exception: EL0 → EL1 transition.
///
/// On SVC:
/// 1. Save current PSTATE to SPSR_EL1
/// 2. Save return address (PC+4) to ELR_EL1
/// 3. Update PSTATE.EL to EL1
/// 4. Stop emulation
fn handle_svc(
    uc: &mut Unicorn<'_, Rc<RefCell<MmioBus>>>,
    addr: u64,
    imm16: u32,
    exc_state: &Rc<RefCell<ExceptionModule>>,
    core_id: usize,
) {
    let from_el = exc_state.borrow().current_el[core_id];

    // Read current PSTATE (contains the CurrentEL field at bits [3:2])
    let pstate = uc.reg_read(RegisterARM64::PSTATE).unwrap_or(0);

    // Save PSTATE to SPSR_EL1 (software-emulated banked register)
    // Save return address to ELR_EL1 (Unicorn exposes this register)
    let return_addr = addr + 4;
    {
        let mut exc = exc_state.borrow_mut();
        exc.spsr_el1[core_id] = pstate;
        exc.current_el[core_id] = EL1;
    }
    let _ = uc.reg_write(RegisterARM64::ELR_EL1, return_addr);

    // Update PSTATE.CurrentEL to EL1 (bits [3:2] = 0b01 << 2 = 0x4)
    let new_pstate = (pstate & !0xC) | ((EL1 as u64) << 2);
    let _ = uc.reg_write(RegisterARM64::PSTATE, new_pstate);

    info!(
        "Exception: SVC #{imm16:#x} at {addr:#x}: EL{from_el} → EL1, \
         SPSR_EL1={pstate:#x}, ELR_EL1={return_addr:#x}"
    );

    // Stop emulation — caller can inspect state and decide next steps
    let _ = uc.emu_stop();
}

/// Handle SMC (Secure Monitor Call) exception: any EL → EL3 transition.
///
/// On SMC:
/// 1. Save current PSTATE to SPSR_EL3
/// 2. Save return address (PC+4) to ELR_EL3
/// 3. Update PSTATE.EL to EL3
/// 4. Stop emulation
fn handle_smc(
    uc: &mut Unicorn<'_, Rc<RefCell<MmioBus>>>,
    addr: u64,
    imm16: u32,
    exc_state: &Rc<RefCell<ExceptionModule>>,
    core_id: usize,
) {
    let from_el = exc_state.borrow().current_el[core_id];

    // Warn for unusual transitions
    if from_el >= EL3 {
        warn!(
            "Exception: SMC #{imm16:#x} at {addr:#x} from EL{from_el} — \
             already at highest exception level"
        );
    }

    // Read current PSTATE
    let pstate = uc.reg_read(RegisterARM64::PSTATE).unwrap_or(0);

    // Save PSTATE to SPSR_EL3 (software-emulated banked register)
    // Save return address to ELR_EL3 (Unicorn exposes this register)
    let return_addr = addr + 4;
    {
        let mut exc = exc_state.borrow_mut();
        exc.spsr_el3[core_id] = pstate;
        exc.current_el[core_id] = EL3;
    }
    let _ = uc.reg_write(RegisterARM64::ELR_EL3, return_addr);

    // Update PSTATE.CurrentEL to EL3 (bits [3:2] = 0b11 << 2 = 0xC)
    let new_pstate = (pstate & !0xC) | ((EL3 as u64) << 2);
    let _ = uc.reg_write(RegisterARM64::PSTATE, new_pstate);

    info!(
        "Exception: SMC #{imm16:#x} at {addr:#x}: EL{from_el} → EL3, \
         SPSR_EL3={pstate:#x}, ELR_EL3={return_addr:#x}"
    );

    // Stop emulation
    let _ = uc.emu_stop();
}
