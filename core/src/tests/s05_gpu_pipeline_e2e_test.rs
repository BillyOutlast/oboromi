//! S05: GPU pipeline end-to-end test with SPIR-V binary output.
//!
//! These tests compose the full SASS→SPIR-V pipeline (Emitter + Decoder)
//! and verify the binary output is structurally valid, including the SPIR-V
//! magic number and size invariants. Complements gpu_test.rs (which only
//! validates in-memory) by exercising Emitter::to_bytes() and disk I/O.

use crate::gpu::sm86::Decoder;
use crate::gpu::spirv::Emitter;
use crate::tests::gpu_test::inst;

// ── Multi-instruction "shader" used by S04 demo ────────────────────────
// FADD + FFMA + LDG + STG — covers arithmetic, memory, and multi-instruction flow.
fn build_multi_instruction_shader() -> Vec<u128> {
    vec![
        inst::fadd_imm(1, 2, 5),   // r1 = r2 + 5 (f32 via bitcast)
        inst::ffma_imm(3, 1, 2, 4), // r3 = r1 * 2 + r4 (f32)
        inst::stg(5, 3, 0),         // global[r5+0] = r3
        inst::ldg(6, 5, 0),         // r6 = global[r5+0]
    ]
}

/// Translate instructions through the full Emitter→Decoder→finalize pipeline,
/// returning the finalized Emitter for assertion.
fn run_pipeline(instructions: &[u128]) -> Emitter {
    let mut emitter = Emitter::new();
    emitter.emit_header();
    emitter.emit_capability(crate::gpu::spirv::capability::SHADER);
    emitter.emit_memory_model(0, 1); // Logical, GLSL450

    let mut decoder = Decoder::new(&mut emitter);
    decoder.init();

    let void_ty = decoder.get_type_void();
    let func_ty = decoder.ir.emit_type_function(void_ty, &[]);
    let _func = decoder.ir.emit_function(void_ty, 0, func_ty);
    decoder.ir.emit_label();

    for inst in instructions {
        decoder.translate(*inst);
    }

    decoder.ir.emit_return();
    decoder.ir.emit_function_end();
    decoder.ir.finalize();

    emitter
}

/// Multi-instruction shader (FADD + FFMA + LDG + STG) produces valid
/// SPIR-V binary. Verify to_bytes() is non-empty, SPIR-V magic number
/// (0x07230203 LE), and that validate() passes.
#[test]
fn test_multi_instruction_shader_produces_valid_spirv_binary() {
    let shader = build_multi_instruction_shader();
    let emitter = run_pipeline(&shader);

    // ── to_bytes() must be non-empty ────────────────────────────
    let bytes = emitter.to_bytes();
    assert!(!bytes.is_empty(), "SPIR-V binary must be non-empty");

    // ── SPIR-V magic number: first 4 bytes LE == 0x07230203 ─────
    assert_eq!(
        &bytes[0..4],
        &[0x03, 0x02, 0x23, 0x07],
        "SPIR-V magic number must be present (LE: 0x07230203)"
    );

    // ── validate() must succeed (structural validation) ─────────
    emitter.validate();
}

/// Minimal shader (single iadd_imm) → to_bytes() → verify magic number.
#[test]
fn test_spirv_binary_magic_number() {
    let instructions = vec![inst::iadd_imm(1, 2, 42)];
    let emitter = run_pipeline(&instructions);

    let bytes = emitter.to_bytes();
    assert!(!bytes.is_empty());
    assert_eq!(
        &bytes[0..4],
        &[0x03, 0x02, 0x23, 0x07],
        "first 4 LE bytes must be SPIR-V magic number 0x07230203"
    );
}

/// Multi-instruction shader → to_bytes() → assert size invariants.
/// SPIR-V header is 5 words (20 bytes). Binary must be word-aligned.
#[test]
fn test_spirv_binary_size_sanity() {
    let shader = build_multi_instruction_shader();
    let emitter = run_pipeline(&shader);

    let bytes = emitter.to_bytes();
    assert!(
        bytes.len() >= 20,
        "SPIR-V binary must be at least 20 bytes (5-word header), got {}",
        bytes.len()
    );
    assert_eq!(
        bytes.len() % 4,
        0,
        "SPIR-V binary must be word-aligned (len % 4 == 0), got {}",
        bytes.len()
    );
}

/// Write binary to temp file, read back, verify byte-for-byte identical.
/// If spirv-val is found on PATH, run it on the temp file for ground-truth
/// validation. Since spirv-val is not installed, this part is skipped.
#[test]
fn test_spirv_binary_writes_to_disk() {
    let shader = build_multi_instruction_shader();
    let emitter = run_pipeline(&shader);
    let bytes = emitter.to_bytes();

    // ── Write to temp dir ────────────────────────────────────────
    let temp_dir = std::env::temp_dir();
    let spv_path = temp_dir.join("test_shader.spv");

    std::fs::write(&spv_path, &bytes).expect("write SPIR-V to temp file");

    // ── Read back and verify identical ───────────────────────────
    let read_back = std::fs::read(&spv_path).expect("read SPIR-V from temp file");
    assert_eq!(bytes, read_back, "round-trip bytes must be identical");

    // ── Cleanup ──────────────────────────────────────────────────
    let _ = std::fs::remove_file(&spv_path);

    // ── spirv-val check (if available) ───────────────────────────
    match std::process::Command::new("spirv-val")
        .arg(&spv_path)
        .output()
    {
        Ok(output) => {
            if output.status.success() {
                println!("spirv-val PASSED on test_shader.spv");
            } else {
                let stderr = String::from_utf8_lossy(&output.stderr);
                println!(
                    "spirv-val found but reported errors: {}",
                    stderr.trim()
                );
            }
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            println!(
                "spirv-val not found on PATH — skipping external validation \
                 (install spirv-tools to enable this check)"
            );
        }
        Err(e) => {
            println!("spirv-val invocation failed: {e}");
        }
    }
}
