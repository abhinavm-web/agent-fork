use agent_fork::CowEngine;
use std::fs;
use std::time::Instant;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let tmp = tempfile::tempdir()?;
    let ws = tmp.path();

    let file_count = 300usize;
    for i in 0..file_count {
        let dir = ws.join(format!("src/mod_{}", i / 20));
        fs::create_dir_all(&dir)?;
        fs::write(
            dir.join(format!("file_{i}.rs")),
            format!("pub fn f_{i}() -> usize {{ {i} }}\n").repeat(20),
        )?;
    }

    let t_init = Instant::now();
    let mut engine = CowEngine::init(ws)?;
    let init_ms = t_init.elapsed().as_secs_f64() * 1000.0;

    // Modify 5 files for Step 1
    for i in 0..5 {
        let p = ws.join(format!("src/mod_0/file_{i}.rs"));
        fs::write(&p, format!("pub fn f_{i}_v2() -> usize {{ {} }}\n", i + 1))?;
    }
    let t_ckpt = Instant::now();
    let m1 = engine.checkpoint("Edit 5 modules")?;
    let ckpt_ms = t_ckpt.elapsed().as_secs_f64() * 1000.0;

    // Fork 2 parallel trajectories from Step 1
    let t_fork = Instant::now();
    engine.fork(1, &["variant-a", "variant-b"])?;
    let fork_ms = t_fork.elapsed().as_secs_f64() * 1000.0;

    // Simulate destructive Step 2 (modify 20 files + delete 10 files)
    for i in 10..30 {
        let p = ws.join(format!("src/mod_{}/file_{i}.rs", i / 20));
        fs::write(&p, "// corrupted\n")?;
    }
    for i in 30..40 {
        let p = ws.join(format!("src/mod_{}/file_{i}.rs", i / 20));
        fs::remove_file(&p)?;
    }
    let _m2 = engine.checkpoint("Destructive agent run")?;

    let t_rewind = Instant::now();
    engine.rewind(1, true)?;
    let rewind_ms = t_rewind.elapsed().as_secs_f64() * 1000.0;

    // Verify exact hash fidelity across all 300 files against Step 1 manifest
    let mut verified_files = 0usize;
    for (rel, entry) in &m1.files {
        let actual_bytes = fs::read(ws.join(rel))?;
        let actual_hash = blake3::hash(&actual_bytes).to_hex().to_string();
        assert_eq!(actual_hash, entry.hash, "hash mismatch for {rel}");
        verified_files += 1;
    }

    println!("=== agent-fork 300-File CoW Benchmark ===");
    println!("Initial 300-file snapshot:      {:.2} ms", init_ms);
    println!("Incremental checkpoint (5 mod): {:.2} ms", ckpt_ms);
    println!("Parallel 2-branch CoW fork:     {:.2} ms (600 files materialized)", fork_ms);
    println!("Rewind after 30 destructive ops:{:.2} ms", rewind_ms);
    println!("Blake3 hash fidelity verified:  {}/{} files (100%)", verified_files, file_count);
    Ok(())
}
