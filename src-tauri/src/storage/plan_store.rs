//! Plans as they are kept in the history (`run_plans`, see `storage::history`): every analysis and
//! every run keeps the plan of each entry, so the activities show afterwards which files were (or
//! would have been) copied, moved or deleted. A plan is stored as JSON, compressed with Deflate:
//! paths repeat a lot and shrink to a fraction. How much is kept in total is limited
//! (`Settings::activity_limit_mb`); the oldest plans go first.

use std::io::{BufWriter, Read, Write};

use anyhow::Context;
use flate2::read::DeflateDecoder;
use flate2::write::DeflateEncoder;
use flate2::Compression;

use crate::sync::planner::Plan;

/// A plan ready to be stored.
#[derive(Debug, Clone)]
pub struct PackedPlan {
    /// `Plan::id`, so a stored plan can be found and replaced (after the probe of a check)
    pub plan_id: String,
    pub data: Vec<u8>,
}

/// Compresses a plan for the history. Fast compression: a plan of a million operations is packed
/// in well under a second (release build) and shrinks to about 8 MB.
pub fn pack(plan: &Plan) -> anyhow::Result<PackedPlan> {
    // serde writes in many small pieces: buffered, the encoder gets them in large blocks
    let mut writer = BufWriter::with_capacity(256 * 1024, DeflateEncoder::new(Vec::new(), Compression::fast()));
    serde_json::to_writer(&mut writer, plan).context("plan not serializable")?;
    writer.flush()?;
    let encoder = writer.into_inner().map_err(|e| e.into_error())?;
    Ok(PackedPlan { plan_id: plan.id.clone(), data: encoder.finish()? })
}

/// Reads back a plan stored by `pack`.
pub fn unpack(data: &[u8]) -> anyhow::Result<Plan> {
    let mut json = Vec::new();
    DeflateDecoder::new(data).read_to_end(&mut json).context("stored plan is damaged")?;
    serde_json::from_slice(&json).context("stored plan is not readable")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Backup;
    use crate::sync::planner::{build_plans, OpKind, Scans};
    use std::fs;
    use std::sync::atomic::AtomicBool;

    #[test]
    fn a_plan_comes_back_as_it_was_stored() {
        let tmp = tempfile::tempdir().unwrap();
        let (src, dst) = (tmp.path().join("src"), tmp.path().join("dst"));
        fs::create_dir_all(src.join("sub")).unwrap();
        fs::create_dir_all(&dst).unwrap();
        fs::write(src.join("sub").join("new.txt"), "new").unwrap();
        fs::write(dst.join("old.txt"), "old").unwrap();
        let backup: Backup = serde_json::from_value(serde_json::json!({
            "id": "b", "name": "B",
            "archives": [{ "id": "a", "source": src, "path": dst, "mode": "mirror" }]
        }))
        .unwrap();
        let plan = build_plans(&backup, 2000, &Scans::plain(&AtomicBool::new(false))).unwrap().remove(0);

        let packed = pack(&plan).unwrap();
        assert_eq!(packed.plan_id, plan.id);
        let back = unpack(&packed.data).unwrap();
        assert_eq!(back.id, plan.id);
        assert_eq!(back.ops.len(), plan.ops.len());
        assert!(back.ops.iter().any(|op| op.kind == OpKind::Copy && op.path == "sub/new.txt"));
        assert!(back.ops.iter().any(|op| op.kind == OpKind::Delete && op.path == "old.txt"));
        assert_eq!(back.summary.copy_files, 1);
        assert_eq!(back.created_at, plan.created_at);
    }

    #[test]
    fn damaged_data_is_an_error() {
        assert!(unpack(b"not a plan").is_err());
    }

    /// How long a plan of a million operations takes and how large it gets
    /// (`cargo test --release -- --ignored large_plan --nocapture`).
    #[test]
    #[ignore]
    fn large_plan_timing() {
        use crate::sync::planner::Op;
        let tmp = tempfile::tempdir().unwrap();
        let backup: Backup = serde_json::from_value(serde_json::json!({
            "id": "b", "name": "B",
            "archives": [{ "id": "a", "source": tmp.path(), "path": tmp.path().join("dst"), "mode": "mirror" }]
        }))
        .unwrap();
        let mut plan = build_plans(&backup, 2000, &Scans::plain(&AtomicBool::new(false))).unwrap().remove(0);
        plan.ops = (0..1_000_000)
            .map(|i| {
                Op::new(OpKind::Copy, format!("Photos/{}/Holiday {}/IMG_{i:06}.jpg", 2000 + i % 25, i % 300), 4_000_000)
            })
            .collect();
        let started = std::time::Instant::now();
        let packed = pack(&plan).unwrap();
        let packed_in = started.elapsed();
        let started = std::time::Instant::now();
        let back = unpack(&packed.data).unwrap();
        println!(
            "1M ops: packed to {:.1} MB in {packed_in:?}, unpacked in {:?}",
            packed.data.len() as f64 / 1e6,
            started.elapsed()
        );
        assert_eq!(back.ops.len(), 1_000_000);
    }
}
