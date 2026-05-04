use std::cell::RefCell;
use std::rc::Rc;

use log::{debug, info, warn};
use unicorn_engine::{Arm64Insn, RegisterARM64, RegisterARM64CP, Unicorn};

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

/// ERET instruction encoding (Exception Return)
/// Restores PC from ELR_ELx and PSTATE from SPSR_ELx, then lowers EL.
const ERET_ENCODING: u32 = 0xD69F03E0;

/// ARM64 exception vector table offsets from VBAR_ELx
/// Synchronous exceptions (SVC, SMC, etc.) use offset 0x400
const VEC_SYNC_OFFSET: u64 = 0x400;
/// IRQ exceptions use offset 0x480
#[allow(dead_code)]
const VEC_IRQ_OFFSET: u64 = 0x480;
/// FIQ exceptions use offset 0x500
#[allow(dead_code)]
const VEC_FIQ_OFFSET: u64 = 0x500;
/// SError exceptions use offset 0x580
#[allow(dead_code)]
const VEC_SERROR_OFFSET: u64 = 0x580;

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

    /// Register SVC/SMC/ERET code hooks and MRS/MSR instruction hooks on a
    /// Unicorn instance.
    ///
    /// The code hook intercepts SVC, SMC, and ERET instructions and performs
    /// manual exception level transitions, since Unicorn does not natively
    /// support ARM64 exception level switching.
    ///
    /// MRS/MSR hooks intercept system register accesses for CurrentEL, DAIF,
    /// SPSR_ELx, and ELR_ELx to route them through the ExceptionModule.
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

        // MRS hook: intercept reads of CurrentEL, SPSR_ELx, ELR_ELx
        let exc_mrs = exc_state.clone();
        let _ = emu.add_insn_sys_hook_arm64(
            Arm64Insn::UC_ARM64_INS_MRS,
            0,
            u64::MAX,
            move |uc, dst_reg, cp_reg| {
                handle_mrs(uc, dst_reg, cp_reg, &exc_mrs, core_id as usize)
            },
        );

        // MSR hook: intercept writes to SPSR_ELx, ELR_ELx, DAIF
        let exc_msr = exc_state.clone();
        let _ = emu.add_insn_sys_hook_arm64(
            Arm64Insn::UC_ARM64_INS_MSR,
            0,
            u64::MAX,
            move |uc, src_reg, cp_reg| {
                handle_msr(uc, src_reg, cp_reg, &exc_msr, core_id as usize)
            },
        );
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

    /// Return the vector base address (VBAR_ELx) for the given exception level.
    /// Returns 0 if the EL has no configured vector table.
    pub fn vector_table(&self, el: u8) -> u64 {
        match el {
            EL1 => self.vbar_el1[0], // caller uses core_id=0 for single-core queries
            EL3 => self.vbar_el3[0],
            _ => 0,
        }
    }

    /// Return the vector base address for a specific core and EL.
    pub fn vector_table_for(&self, core_id: usize, el: u8) -> u64 {
        if core_id >= 8 {
            return 0;
        }
        match el {
            EL1 => self.vbar_el1[core_id],
            EL3 => self.vbar_el3[core_id],
            _ => 0,
        }
    }
}

// --- Free functions with generic lifetime for hook callbacks ---

/// Check if an instruction at the given address is SVC, SMC, or ERET,
/// and if so, perform the appropriate exception transition.
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
    } else if instr == ERET_ENCODING {
        handle_eret(uc, addr, exc_state, core_id);
    }
}

/// Handle SVC (Supervisor Call) exception: EL0 → EL1 transition.
///
/// On SVC:
/// 1. Save current PSTATE to SPSR_EL1
/// 2. Save return address (PC+4) to ELR_EL1
/// 3. Update PSTATE.EL to EL1
/// 4. Set PC to VBAR_EL1 + synchronous offset (0x400)
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
    let vbar;
    {
        let mut exc = exc_state.borrow_mut();
        exc.spsr_el1[core_id] = pstate;
        exc.current_el[core_id] = EL1;
        vbar = exc.vbar_el1[core_id];
    }
    let _ = uc.reg_write(RegisterARM64::ELR_EL1, return_addr);

    // Update PSTATE.CurrentEL to EL1 (bits [3:2] = 0b01 << 2 = 0x4)
    let new_pstate = (pstate & !0xC) | ((EL1 as u64) << 2);
    let _ = uc.reg_write(RegisterARM64::PSTATE, new_pstate);

    // Jump to vector table: VBAR_EL1 + synchronous offset
    let handler_addr = vbar.wrapping_add(VEC_SYNC_OFFSET);
    if vbar != 0 {
        // VBAR is configured — jump to handler
        let _ = uc.set_pc(handler_addr);
        info!(
            "Exception: SVC #{imm16:#x} at {addr:#x}: EL{from_el} → EL1, \
             SPSR_EL1={pstate:#x}, ELR_EL1={return_addr:#x}, \
             jumping to handler {handler_addr:#x}"
        );
    } else {
        // No vector table configured — stop emulation
        info!(
            "Exception: SVC #{imm16:#x} at {addr:#x}: EL{from_el} → EL1, \
             SPSR_EL1={pstate:#x}, ELR_EL1={return_addr:#x}, \
             no vector table — stopping"
        );
        let _ = uc.emu_stop();
    }
}

/// Handle SMC (Secure Monitor Call) exception: any EL → EL3 transition.
///
/// On SMC:
/// 1. Save current PSTATE to SPSR_EL3
/// 2. Save return address (PC+4) to ELR_EL3
/// 3. Update PSTATE.EL to EL3
/// 4. Set PC to VBAR_EL3 + synchronous offset (0x400)
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
    let vbar;
    {
        let mut exc = exc_state.borrow_mut();
        exc.spsr_el3[core_id] = pstate;
        exc.current_el[core_id] = EL3;
        vbar = exc.vbar_el3[core_id];
    }
    let _ = uc.reg_write(RegisterARM64::ELR_EL3, return_addr);

    // Update PSTATE.CurrentEL to EL3 (bits [3:2] = 0b11 << 2 = 0xC)
    let new_pstate = (pstate & !0xC) | ((EL3 as u64) << 2);
    let _ = uc.reg_write(RegisterARM64::PSTATE, new_pstate);

    // Jump to vector table: VBAR_EL3 + synchronous offset
    let handler_addr = vbar.wrapping_add(VEC_SYNC_OFFSET);
    if vbar != 0 {
        // VBAR is configured — jump to handler
        let _ = uc.set_pc(handler_addr);
        info!(
            "Exception: SMC #{imm16:#x} at {addr:#x}: EL{from_el} → EL3, \
             SPSR_EL3={pstate:#x}, ELR_EL3={return_addr:#x}, \
             jumping to handler {handler_addr:#x}"
        );
    } else {
        // No vector table configured — stop emulation
        info!(
            "Exception: SMC #{imm16:#x} at {addr:#x}: EL{from_el} → EL3, \
             SPSR_EL3={pstate:#x}, ELR_EL3={return_addr:#x}, \
             no vector table — stopping"
        );
        let _ = uc.emu_stop();
    }
}

/// Handle ERET (Exception Return): restore state and return to lower EL.
///
/// On ERET:
/// 1. Read ELR_ELx (based on current EL) for return address
/// 2. Read SPSR_ELx for saved PSTATE
/// 3. Restore PC from ELR_ELx
/// 4. Restore PSTATE from SPSR_ELx (which lowers the EL)
/// 5. Stop emulation
fn handle_eret(
    uc: &mut Unicorn<'_, Rc<RefCell<MmioBus>>>,
    addr: u64,
    exc_state: &Rc<RefCell<ExceptionModule>>,
    core_id: usize,
) {
    let from_el = exc_state.borrow().current_el[core_id];

    // Read ELR and SPSR based on current exception level
    let (elr, spsr, spsr_name) = match from_el {
        EL3 => {
            let elr = uc.reg_read(RegisterARM64::ELR_EL3).unwrap_or(0);
            let spsr = exc_state.borrow().spsr_el3[core_id];
            (elr, spsr, "SPSR_EL3")
        }
        EL1 => {
            let elr = uc.reg_read(RegisterARM64::ELR_EL1).unwrap_or(0);
            let spsr = exc_state.borrow().spsr_el1[core_id];
            (elr, spsr, "SPSR_EL1")
        }
        _ => {
            warn!(
                "ERET at {addr:#x} from EL{from_el} — unexpected exception level, \
                 attempting EL1 path"
            );
            let elr = uc.reg_read(RegisterARM64::ELR_EL1).unwrap_or(0);
            let spsr = exc_state.borrow().spsr_el1[core_id];
            (elr, spsr, "SPSR_EL1")
        }
    };

    // Determine target EL from SPSR (bits [3:2])
    let to_el = ((spsr >> 2) & 0x3) as u8;

    if from_el == EL0 {
        warn!(
            "ERET at {addr:#x} from EL{from_el} — ERET should not be executed \
             from EL0, restoring anyway"
        );
    }

    // Restore PSTATE from SPSR
    let _ = uc.reg_write(RegisterARM64::PSTATE, spsr);

    // Restore PC from ELR
    let _ = uc.set_pc(elr);

    // Update software EL tracking
    {
        let mut exc = exc_state.borrow_mut();
        exc.current_el[core_id] = to_el;
    }

    debug!(
        "Exception: ERET at {addr:#x}: EL{from_el} → EL{to_el}, \
         {spsr_name}={spsr:#x}, restored PC={elr:#x}"
    );

    // Stop emulation after ERET so caller can inspect state
    let _ = uc.emu_stop();
}

/// Handle MRS instruction: read a system register into a general-purpose register.
///
/// Called by Unicorn's instruction hook when an MRS instruction is executed.
/// Returns `true` to skip Unicorn's default handling (we handle the register
/// read ourselves), or `false` to let Unicorn handle it.
fn handle_mrs(
    uc: &mut Unicorn<'_, Rc<RefCell<MmioBus>>>,
    dst_reg: RegisterARM64,
    cp_reg: &RegisterARM64CP,
    exc_state: &Rc<RefCell<ExceptionModule>>,
    core_id: usize,
) -> bool {
    let op0 = cp_reg.op0;
    let op1 = cp_reg.op1;
    let crn = cp_reg.crn;
    let crm = cp_reg.crm;
    let op2 = cp_reg.op2;

    let value = identify_and_read_sys_reg(op0, op1, crn, crm, op2, exc_state, core_id, uc);

    if let Some(val) = value {
        let _ = uc.reg_write(dst_reg, val);
        debug!(
            "MRS hook: read sysreg({op0},{op1},{crn},{crm},{op2}) = {val:#x} → {:?}",
            dst_reg
        );
        true // Skip Unicorn's default handling
    } else {
        false // Let Unicorn handle unknown registers
    }
}

/// Handle MSR instruction: write a general-purpose register value to a system register.
///
/// Called by Unicorn's instruction hook when an MSR instruction is executed.
/// Returns `true` to skip Unicorn's default handling, or `false` to let
/// Unicorn handle it.
fn handle_msr(
    uc: &mut Unicorn<'_, Rc<RefCell<MmioBus>>>,
    src_reg: RegisterARM64,
    cp_reg: &RegisterARM64CP,
    exc_state: &Rc<RefCell<ExceptionModule>>,
    core_id: usize,
) -> bool {
    let op0 = cp_reg.op0;
    let op1 = cp_reg.op1;
    let crn = cp_reg.crn;
    let crm = cp_reg.crm;
    let op2 = cp_reg.op2;

    let value = uc.reg_read(src_reg).unwrap_or(0);

    let handled = identify_and_write_sys_reg(op0, op1, crn, crm, op2, value, exc_state, core_id, uc);

    if handled {
        debug!(
            "MSR hook: wrote {value:#x} → sysreg({op0},{op1},{crn},{crm},{op2}) from {:?}",
            src_reg
        );
        true // Skip Unicorn's default handling
    } else {
        false // Let Unicorn handle unknown registers
    }
}

/// Identify a system register by its op0/op1/CRn/CRm/op2 encoding and read
/// its value from the ExceptionModule or Unicorn.
///
/// Returns `Some(value)` if the register is recognized, `None` otherwise.
fn identify_and_read_sys_reg(
    op0: u32,
    op1: u32,
    crn: u32,
    crm: u32,
    op2: u32,
    exc_state: &Rc<RefCell<ExceptionModule>>,
    core_id: usize,
    uc: &Unicorn<'_, Rc<RefCell<MmioBus>>>,
) -> Option<u64> {
    match (op0, op1, crn, crm, op2) {
        // CurrentEL: op0=3, op1=0, CRn=4, CRm=2, op2=2 (read-only)
        (3, 0, 4, 2, 2) => {
            let el = exc_state.borrow().current_el[core_id];
            Some((el as u64) << 2)
        }
        // SPSR_EL1: op0=3, op1=0, CRn=4, CRm=0, op2=0
        (3, 0, 4, 0, 0) => Some(exc_state.borrow().spsr_el1[core_id]),
        // SPSR_EL3: op0=3, op1=6, CRn=4, CRm=0, op2=0
        (3, 6, 4, 0, 0) => Some(exc_state.borrow().spsr_el3[core_id]),
        // ELR_EL1: op0=3, op1=0, CRn=4, CRm=0, op2=1
        (3, 0, 4, 0, 1) => Some(uc.reg_read(RegisterARM64::ELR_EL1).unwrap_or(0)),
        // ELR_EL3: op0=3, op1=6, CRn=4, CRm=0, op2=1
        (3, 6, 4, 0, 1) => Some(uc.reg_read(RegisterARM64::ELR_EL3).unwrap_or(0)),
        // DAIFSet: op0=3, op1=3, CRn=4, CRm=3, op2=1
        // Reading DAIFSet returns current DAIF mask
        (3, 3, 4, 3, 1) => Some(exc_state.borrow().daif[core_id]),
        // DAIFClr: op0=3, op1=3, CRn=4, CRm=3, op2=0
        // Reading DAIFClr also returns current DAIF mask
        (3, 3, 4, 3, 0) => Some(exc_state.borrow().daif[core_id]),
        _ => None,
    }
}

/// Identify a system register by its op0/op1/CRn/CRm/op2 encoding and write
/// a value to it through the ExceptionModule or Unicorn.
///
/// Returns `true` if the register is recognized and written, `false` otherwise.
fn identify_and_write_sys_reg(
    op0: u32,
    op1: u32,
    crn: u32,
    crm: u32,
    op2: u32,
    value: u64,
    exc_state: &Rc<RefCell<ExceptionModule>>,
    core_id: usize,
    uc: &mut Unicorn<'_, Rc<RefCell<MmioBus>>>,
) -> bool {
    match (op0, op1, crn, crm, op2) {
        // CurrentEL is read-only — writes are ignored
        (3, 0, 4, 2, 2) => {
            warn!("MSR: attempt to write read-only CurrentEL register — ignored");
            true
        }
        // SPSR_EL1: op0=3, op1=0, CRn=4, CRm=0, op2=0
        (3, 0, 4, 0, 0) => {
            exc_state.borrow_mut().spsr_el1[core_id] = value;
            true
        }
        // SPSR_EL3: op0=3, op1=6, CRn=4, CRm=0, op2=0
        (3, 6, 4, 0, 0) => {
            exc_state.borrow_mut().spsr_el3[core_id] = value;
            true
        }
        // ELR_EL1: op0=3, op1=0, CRn=4, CRm=0, op2=1
        (3, 0, 4, 0, 1) => {
            let _ = uc.reg_write(RegisterARM64::ELR_EL1, value);
            true
        }
        // ELR_EL3: op0=3, op1=6, CRn=4, CRm=0, op2=1
        (3, 6, 4, 0, 1) => {
            let _ = uc.reg_write(RegisterARM64::ELR_EL3, value);
            true
        }
        // DAIFSet: op0=3, op1=3, CRn=4, CRm=3, op2=1
        // Write sets bits in DAIF (OR with existing)
        (3, 3, 4, 3, 1) => {
            let mut exc = exc_state.borrow_mut();
            exc.daif[core_id] |= (value << 6) & 0x3C0;
            true
        }
        // DAIFClr: op0=3, op1=3, CRn=4, CRm=3, op2=0
        // Write clears bits in DAIF (AND NOT)
        (3, 3, 4, 3, 0) => {
            let mut exc = exc_state.borrow_mut();
            exc.daif[core_id] &= !((value << 6) & 0x3C0);
            true
        }
        _ => false,
    }
}
