use crate::presentation::shell::Shell;
use serde_json::{Value, json};

pub fn snapshot(view: &Shell) -> Value {
    let (sources, thumbnails) = view.foundry.cache_stats;
    let images = retinue_render::painter::image_memory();
    let rasters = view.render_memory();
    json!({
        "process": process(), "glibc_heap": glibc_heap(),
        "blend2d": blend2d(),
        "native_images": images.images, "native_pixel_bytes": images.pixel_bytes,
        "decoded_source_bytes": sources.live_bytes, "thumbnail_bytes": thumbnails.live_bytes,
        "active_thumbnail_bytes": thumbnails.retained_bytes,
        "foundry_resource_bytes": view.foundry.memory().0,
        "page_raster_bytes": view.pages.ui.raster_memory().0,
        "rail_raster_bytes": view.pages.rail.raster_memory().0,
        "render_rasters": {
            "selected_bytes": rasters.selected_bytes,
            "warm_bytes": rasters.warm_bytes,
            "retired_bytes": rasters.retired_bytes,
            "pending_bytes": rasters.pending_bytes,
            "live_bytes": rasters.live_bytes,
            "peak_limit_bytes": rasters.peak_limit_bytes,
            "warm_entries": rasters.warm_entries,
            "expired_entries": rasters.expired_entries,
        },
        "render_analysis": {
            "jobs": rasters.analysis.jobs,
            "reserved_bytes": rasters.analysis.reserved_bytes,
            "running": rasters.analysis.running,
            "submitted": rasters.analysis.submitted,
            "published": rasters.analysis.published,
            "discarded": rasters.analysis.discarded,
            "failures": rasters.analysis.failures,
        },
    })
}

pub fn blend2d() -> Value {
    let Ok(pools) = retinue_render::painter::runtime_resources() else {
        return Value::Null;
    };
    json!({
        "jit_used_bytes": pools.jit_used_bytes, "jit_reserved_bytes": pools.jit_reserved_bytes,
        "zero_pool_used_bytes": pools.zero_pool_used_bytes,
        "zero_pool_reserved_bytes": pools.zero_pool_reserved_bytes, "pipelines": pools.pipelines,
    })
}

pub fn process() -> Value {
    let Ok(text) = std::fs::read_to_string("/proc/self/smaps_rollup") else {
        return Value::Null;
    };
    Value::Object(
        text.lines()
            .filter_map(|line| {
                let (key, rest) = line.split_once(':')?;
                let mut fields = rest.split_whitespace();
                let bytes = fields.next()?.parse::<u64>().ok()?.checked_mul(1024)?;
                if fields.next()? != "kB" {
                    return None;
                }
                Some((key.to_owned(), json!(bytes)))
            })
            .collect(),
    )
}

pub fn glibc_heap() -> Value {
    #[cfg(all(target_os = "linux", target_env = "gnu"))]
    {
        #[repr(C)]
        struct MallInfo {
            arena: usize,
            ordblks: usize,
            smblks: usize,
            hblks: usize,
            hblkhd: usize,
            usmblks: usize,
            fsmblks: usize,
            uordblks: usize,
            fordblks: usize,
            keepcost: usize,
        }
        unsafe extern "C" {
            fn mallinfo2() -> MallInfo;
        }
        // glibc's read-only allocator snapshot; free arena space is not necessarily resident.
        let info = unsafe { mallinfo2() };
        json!({"in_use_bytes": info.uordblks + info.hblkhd, "free_arena_bytes": info.fordblks,
            "arena_bytes": info.arena, "mapped_heap_bytes": info.hblkhd})
    }
    #[cfg(not(all(target_os = "linux", target_env = "gnu")))]
    {
        Value::Null
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn process_counts_are_bytes_and_native_pool_counts_are_consistent() {
        let process = super::process();
        if cfg!(target_os = "linux") {
            assert!(process["Rss"].as_u64().unwrap() > 1024 * 1024);
            assert!(
                process
                    .as_object()
                    .unwrap()
                    .keys()
                    .all(|key| !key.contains(' '))
            );
        }
        let pools = retinue_render::painter::runtime_resources().unwrap();
        assert!(pools.jit_used_bytes <= pools.jit_reserved_bytes);
        assert!(pools.zero_pool_used_bytes <= pools.zero_pool_reserved_bytes);
    }
}
