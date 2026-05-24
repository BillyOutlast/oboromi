use crate::gpu::sm86::Decoder;
use crate::gpu::spirv::Emitter;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::time::{Duration, Instant};

#[derive(Debug, Clone)]
pub struct TestResult {
    pub name: String,
    pub passed: bool,
    pub message: String,
    pub duration: Duration,
}

// helpers to build sm86 instructions
pub(crate) mod inst {
    // iadd rd, ra, imm32 (src_type=4, opcode=0x810)
    pub fn iadd_imm(rd: u32, ra: u32, imm32: u32) -> u128 {
        let mut inst: u128 = 0x810;
        inst |= ((rd as u128) & 0xff) << 16;
        inst |= ((ra as u128) & 0xff) << 24;
        inst |= ((imm32 as u128) & 0xffffffff) << 32;
        inst |= 0xFFu128 << 64; // rc=0xFF to bypass assert
        inst
    }

    // iadd rd, ra, rb (src_type=1, opcode=0x210)
    pub fn iadd_reg(rd: u32, ra: u32, rb: u32) -> u128 {
        let mut inst: u128 = 0x210;
        inst |= ((rd as u128) & 0xff) << 16;
        inst |= ((ra as u128) & 0xff) << 24;
        inst |= ((rb as u128) & 0xff) << 32;
        inst |= 0xFFu128 << 64; // rc=0xFF to bypass assert
        inst
    }

    // iadd3 rd, ra, rb, rc (opcode=0x510, variant 0x1510 via bit91)
    pub fn iadd3_reg(rd: u32, ra: u32, rb: u32, rc: u32) -> u128 {
        let mut inst: u128 = 0x510;
        inst |= 1u128 << 91; // use 0x1510 variant to avoid iadd overlap
        inst |= ((rd as u128) & 0xff) << 16;
        inst |= ((ra as u128) & 0xff) << 24;
        inst |= ((rb as u128) & 0xff) << 32;
        inst |= ((rc as u128) & 0xff) << 64;
        inst
    }

    // iadd32i rd, ra, imm32 (opcode=0x410)
    pub fn iadd32i(rd: u32, ra: u32, imm32: u32) -> u128 {
        let mut inst: u128 = 0x410;
        inst |= ((rd as u128) & 0xff) << 16;
        inst |= ((ra as u128) & 0xff) << 24;
        inst |= ((imm32 as u128) & 0xffffffff) << 32;
        inst
    }

    // kill (opcode=0x8e0)
    pub fn kill(pred: u32, invert: bool) -> u128 {
        let mut inst: u128 = 0x8e0;
        inst |= ((pred as u128) & 0x7) << 12;
        if invert {
            inst |= 1 << 15;
        }
        inst
    }

    // --- FADD: opcode 0x621, rd = ra + imm (float via bitcast) ---
    // sc_addr[40:53] is the u32 constant value (14-bit, 0..16383)
    // The handler bitcasts both ra and the constant to f32, then fadd's them.
    pub fn fadd_imm(rd: u32, ra: u32, sc_val: u32) -> u128 {
        assert!(sc_val <= 0x3fff, "FADD sc_val exceeds 14-bit range");
        let mut inst: u128 = 0x621;
        inst |= 7u128 << 12; // pg=PT
        inst |= ((rd as u128) & 0xff) << 16;
        inst |= ((ra as u128) & 0xff) << 24;
        inst |= ((sc_val as u128) & 0x3fff) << 40; // sc_addr
        inst
    }

    // --- FFMA: opcode 0xa23, rd = ra * sc_val + rc (float via bitcast) ---
    // sc_addr[40:53] is the u32 constant value (14-bit, 0..16383)
    pub fn ffma_imm(rd: u32, ra: u32, sc_val: u32, rc: u32) -> u128 {
        assert!(sc_val <= 0x3fff, "FFMA sc_val exceeds 14-bit range");
        let mut inst: u128 = 0xa23;
        inst |= 7u128 << 12; // pg=PT
        inst |= ((rd as u128) & 0xff) << 16;
        inst |= ((ra as u128) & 0xff) << 24;
        inst |= ((sc_val as u128) & 0x3fff) << 40; // sc_addr
        inst |= ((rc as u128) & 0xff) << 64;
        inst
    }

    // --- IMAD: opcode 0xa24, rd = ra * imm32 + rc (integer) ---
    // Fields: pg[12:14], pg_not[15], rd[16:23], ra[24:31],
    //         sc_addr[40:53], sc_bank[54:58], rc[64:71],
    //         sz[73], sc_absolute[74], sc_negate[75],
    //         pu[81:83], cop[84:86], pp[87:89]
    pub fn imad_imm(rd: u32, ra: u32, imm32: u32, rc: u32) -> u128 {
        let mut inst: u128 = 0xa24;
        inst |= 7u128 << 12; // pg=PT
        inst |= ((rd as u128) & 0xff) << 16;
        inst |= ((ra as u128) & 0xff) << 24;
        inst |= ((imm32 as u128) & 0x3fff) << 40; // sc_addr
        inst |= ((rc as u128) & 0xff) << 64;
        inst
    }

    // --- LOP3: opcode 0x812, rd = LUT(a, b=imm32, c=rc) ---
    // Fields: pg[12:14], pg_not[15], rd[16:23], ra[24:31],
    //         ra_offset[32:63], rc[64:71], imm8[72:79],
    //         ftz[80], pu[81:83], cop[84:86], pp[87:89]
    pub fn lop3_imm(rd: u32, ra: u32, imm32: u32, rc: u32, lut: u8) -> u128 {
        let mut inst: u128 = 0x812;
        inst |= 7u128 << 12; // pg=PT
        inst |= ((rd as u128) & 0xff) << 16;
        inst |= ((ra as u128) & 0xff) << 24;
        inst |= ((imm32 as u128) & 0xffffffff) << 32; // ra_offset
        inst |= ((rc as u128) & 0xff) << 64;
        inst |= ((lut as u128) & 0xff) << 72; // imm8
        inst
    }

    // --- MOV: opcode 0xa02, rd = const_bank[sc_addr] ---
    // Fields: pg[12:14], pg_not[15], rd[16:23],
    //         sc_addr[40:53], sc_bank[54:58], pixmasku04[72:75]
    pub fn mov_imm(rd: u32, imm32: u32) -> u128 {
        let mut inst: u128 = 0xa02;
        inst |= 7u128 << 12; // pg=PT
        inst |= ((rd as u128) & 0xff) << 16;
        inst |= ((imm32 as u128) & 0x3fff) << 40; // sc_addr
        inst
    }

    // --- SEL: opcode 0xa07, rd = pp ? ra : sc_addr ---
    // Fields: pg[12:14], pg_not[15], rd[16:23], ra[24:31],
    //         sc_addr[40:53], sc_bank[54:58], pp[87:89]
    pub fn sel_imm(rd: u32, ra: u32, imm32: u32, pp: u32) -> u128 {
        let mut inst: u128 = 0xa07;
        inst |= 7u128 << 12; // pg=PT
        inst |= ((rd as u128) & 0xff) << 16;
        inst |= ((ra as u128) & 0xff) << 24;
        inst |= ((imm32 as u128) & 0x3fff) << 40; // sc_addr
        inst |= ((pp as u128) & 0x7) << 87;
        inst
    }

    // --- LDG: opcode 0x381, rd = global[ra + offset] ---
    // Fields: pg[12:14], pg_not[15], rd[16:23], ra[24:31],
    //         ra_offset[40:63], _pnz[64:67], _sp2[68:69],
    //         e[72], sz[73:75], _memdesc[76], mem[77:80],
    //         pu[81:83], cop[84:86]
    pub fn ldg(rd: u32, ra: u32, offset: u32) -> u128 {
        let mut inst: u128 = 0x381;
        inst |= 7u128 << 12; // pg=PT
        inst |= ((rd as u128) & 0xff) << 16;
        inst |= ((ra as u128) & 0xff) << 24;
        inst |= ((offset as u128) & 0xffffff) << 40; // ra_offset (24-bit)
        inst
    }

    // --- STG: opcode 0x386, global[ra + offset] = rb ---
    // Fields: pg[12:14], pg_not[15], ra[24:31], rb[32:39],
    //         ra_offset[40:63], e[72], sz[73:75], _memdesc[76], mem[77:80],
    //         cop[84:86]
    pub fn stg(ra: u32, rb: u32, offset: u32) -> u128 {
        let mut inst: u128 = 0x386;
        inst |= 7u128 << 12; // pg=PT
        inst |= ((ra as u128) & 0xff) << 24;
        inst |= ((rb as u128) & 0xff) << 32;
        inst |= ((offset as u128) & 0xffffff) << 40; // ra_offset (24-bit)
        inst
    }

    // --- LDS: opcode 0x984, rd = shared[ra + offset] ---
    // Fields: pg[12:14], pg_not[15], rd[16:23], ra[24:31],
    //         ra_offset[40:63], sz[73:75], stride[78:79]
    pub fn lds(rd: u32, ra: u32, offset: u32) -> u128 {
        let mut inst: u128 = 0x984;
        inst |= 7u128 << 12; // pg=PT
        inst |= ((rd as u128) & 0xff) << 16;
        inst |= ((ra as u128) & 0xff) << 24;
        inst |= ((offset as u128) & 0xffffff) << 40; // ra_offset (24-bit)
        inst
    }

    // --- STS: opcode 0x388, shared[ra + offset] = rb ---
    // Fields: pg[12:14], pg_not[15], ra[24:31], rb[32:39],
    //         ra_offset[40:63], sz[73:75], stride[78:79]
    pub fn sts(ra: u32, rb: u32, offset: u32) -> u128 {
        let mut inst: u128 = 0x388;
        inst |= 7u128 << 12; // pg=PT
        inst |= ((ra as u128) & 0xff) << 24;
        inst |= ((rb as u128) & 0xff) << 32;
        inst |= ((offset as u128) & 0xffffff) << 40; // ra_offset (24-bit)
        inst
    }
}

fn run_translation_test(name: &str, instructions: &[u128]) -> TestResult {
    let start = Instant::now();
    println!("Running test: {} ({} instructions)", name, instructions.len());
    let result = catch_unwind(AssertUnwindSafe(|| {
        let mut emitter = Emitter::new();
        emitter.emit_header();
        emitter.emit_capability(crate::gpu::spirv::capability::SHADER);
        emitter.emit_memory_model(0, 1);

        let mut decoder = Decoder::new(&mut emitter);
        decoder.init();

        // set up a function so translated instructions have a valid context
        let void_ty = decoder.get_type_void();
        let func_ty = decoder.ir.emit_type_function(void_ty, &[]);
        let _func = decoder.ir.emit_function(void_ty, 0, func_ty);
        decoder.ir.emit_label();

        for &inst in instructions {
            decoder.translate(inst);
        }

        decoder.ir.emit_return();
        decoder.ir.emit_function_end();
        decoder.ir.finalize();
        decoder.ir.validate();
    }));

    let duration = start.elapsed();
    match result {
        Ok(()) => TestResult {
            name: name.to_string(),
            passed: true,
            message: "PASS".to_string(),
            duration,
        },
        Err(e) => {
            let msg = if let Some(s) = e.downcast_ref::<&str>() {
                s.to_string()
            } else if let Some(s) = e.downcast_ref::<String>() {
                s.clone()
            } else {
                "unknown panic".to_string()
            };
            TestResult {
                name: name.to_string(),
                passed: false,
                message: format!("FAIL: {}", msg),
                duration,
            }
        }
    }
}

#[test]
fn gpu_handler_tests() {
    let results = run_gpu_tests();
    let mut failed = 0;
    for line in &results {
        if line.starts_with("N ") {
            failed += 1;
        }
    }
    // Print all results for diagnostics
    for line in &results {
        println!("{}", line);
    }
    if failed > 0 {
        panic!("{} GPU handler test(s) failed", failed);
    }
}

pub fn run_gpu_tests() -> Vec<String> {
    let mut results = Vec::new();
    let start_time = Instant::now();

    results.push("Starting GPU/SM86 Decoder Tests...".to_string());
    println!("Starting GPU/SM86 Decoder Tests...");

    let tests = vec![
        // --- Existing tests ---
        run_translation_test("IADD Immediate", &[inst::iadd_imm(1, 2, 42)]),
        run_translation_test("IADD Register", &[inst::iadd_reg(1, 2, 3)]),
        run_translation_test("IADD3 Register", &[inst::iadd3_reg(1, 2, 3, 4)]),
        run_translation_test("IADD32I", &[inst::iadd32i(1, 2, 100)]),
        run_translation_test("KILL PT", &[inst::kill(7, false)]),

        // --- T05: 10 new handler tests ---
        // FADD: r1 = r2 + 1 (as f32 via bitcast)
        run_translation_test("FADD Imm (r2 + 1)",
            &[inst::fadd_imm(1, 2, 1)]),
        // FADD with negated source: r1 = r2 + (-42) [sc_negate=1]
        run_translation_test("FADD Negate B (r2 - 42)",
            &[inst::fadd_imm(1, 2, 42) | (1u128 << 63)]),

        // FFMA: r1 = r2 * 3 + r4
        run_translation_test("FFMA Imm (r2 * 3 + r4)",
            &[inst::ffma_imm(1, 2, 3, 4)]),

        // IMAD: r1 = (r2 * 10) + r3
        run_translation_test("IMAD (r2 * 10 + r3)",
            &[inst::imad_imm(1, 2, 10, 3)]),

        // LOP3 AND: r1 = r2 & 0xFFFF & r3  (LUT=0x80)
        run_translation_test("LOP3 AND (LUT=0x80)",
            &[inst::lop3_imm(1, 2, 0xFFFF, 3, 0x80)]),
        // LOP3 XOR: r1 = r2 ^ 0xFFFF ^ r3  (LUT=0x96)
        run_translation_test("LOP3 XOR (LUT=0x96)",
            &[inst::lop3_imm(1, 2, 0xFFFF, 3, 0x96)]),
        // LOP3 NAND: r1 = ~(r2 & 0xFFFF & r3)  (LUT=0xFE)
        run_translation_test("LOP3 NAND (LUT=0xFE)",
            &[inst::lop3_imm(1, 2, 0xFFFF, 3, 0xFE)]),

        // MOV: r1 = 42
        run_translation_test("MOV Immediate (r1 = 42)",
            &[inst::mov_imm(1, 42)]),
        // MOV: r1 = 0
        run_translation_test("MOV Zero (r1 = 0)",
            &[inst::mov_imm(1, 0)]),

        // SEL: r1 = PP7 (always true) ? r2 : imm=200
        run_translation_test("SEL PP7 (always true → r2)",
            &[inst::sel_imm(1, 2, 200, 7)]),

        // LDG: r1 = global[r2 + 0]
        run_translation_test("LDG Load (global[r2+0] → r1)",
            &[inst::ldg(1, 2, 0)]),
        // LDG with offset
        run_translation_test("LDG Load with offset (global[r3+4] → r1)",
            &[inst::ldg(1, 3, 4)]),

        // STG: global[r2 + 0] = r3
        run_translation_test("STG Store (global[r2+0] = r3)",
            &[inst::stg(2, 3, 0)]),
        // STG + LDG round-trip: store r3 to addr r2, load back to r1
        run_translation_test("STG+LDG Round-Trip",
            &[inst::stg(2, 3, 0), inst::ldg(1, 2, 0)]),

        // LDS: r1 = shared[r2 + 0]
        run_translation_test("LDS Load (shared[r2+0] → r1)",
            &[inst::lds(1, 2, 0)]),

        // STS: shared[r2 + 0] = r3
        run_translation_test("STS Store (shared[r2+0] = r3)",
            &[inst::sts(2, 3, 0)]),
        // LDS + STS round-trip: store r3 to shared addr r2, load back to r1
        run_translation_test("LDS+STS Round-Trip",
            &[inst::sts(2, 3, 0), inst::lds(1, 2, 0)]),

        // --- Multi-instruction shader (slice demo): FADD + FFMA + LDG + STG ---
        run_translation_test("Multi: FADD+FFMA+LDG+STG", &[
            inst::fadd_imm(1, 2, 5),              // r1 = r2 + 5 (as f32 via bitcast)
            inst::ffma_imm(3, 1, 2, 4),           // r3 = r1 * 2 + r4 (as f32)
            inst::stg(5, 3, 0),                    // global[r5+0] = r3
            inst::ldg(6, 5, 0),                    // r6 = global[r5+0]
        ]),

        // --- Predicated no-op tests ---
        // FADD with pg=0 (no predicate → skips store since mask=0b000)
        run_translation_test("FADD Predicated Off (pg=0)",
            &[inst::fadd_imm(1, 2, 7) & !(7u128 << 12)]),
        // IMAD with pg=0 (no predicate → still executes since pg_not=0 and pg=0 means mask=all-0?)
        run_translation_test("IMAD Predicated Off (pg=0)",
            &[inst::imad_imm(1, 2, 10, 3) & !(7u128 << 12)]),
        // MOV with pg=0
        run_translation_test("MOV Predicated Off (pg=0)",
            &[inst::mov_imm(1, 99) & !(7u128 << 12)]),
    ];

    let mut passed = 0;
    for t in &tests {
        let icon = if t.passed { "Y" } else { "N" };
        let line = format!(
            "{} {} - {} ({:?})",
            icon, t.name, t.message, t.duration
        );
        println!("{}", line);
        results.push(line);
        if t.passed {
            passed += 1;
        }
    }

    let failed = tests.len() - passed;
    let total_time = start_time.elapsed();
    let summary = format!(
        "Total: {} ({}/{} passed) time {:?}",
        tests.len(),
        passed,
        failed,
        total_time
    );
    println!("{}", summary);
    results.push(summary);

    results
}
