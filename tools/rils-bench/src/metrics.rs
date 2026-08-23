use std::{
    alloc::{GlobalAlloc, Layout, System},
    sync::atomic::{AtomicU64, Ordering},
};

pub(crate) struct TrackingAllocator;

static ALLOCATION_COUNT: AtomicU64 = AtomicU64::new(0);
static ALLOCATED_BYTES: AtomicU64 = AtomicU64::new(0);
static DEALLOCATED_BYTES: AtomicU64 = AtomicU64::new(0);
static LIVE_BYTES: AtomicU64 = AtomicU64::new(0);
static PEAK_LIVE_BYTES: AtomicU64 = AtomicU64::new(0);

unsafe impl GlobalAlloc for TrackingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let pointer = unsafe { System.alloc(layout) };
        if !pointer.is_null() {
            record_allocation(layout.size() as u64);
        }
        pointer
    }

    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        unsafe { System.dealloc(pointer, layout) };
        record_deallocation(layout.size() as u64);
    }

    unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        let replacement = unsafe { System.realloc(pointer, layout, new_size) };
        if !replacement.is_null() {
            let old_size = layout.size() as u64;
            let new_size = new_size as u64;
            if new_size >= old_size {
                record_allocation(new_size - old_size);
            } else {
                record_deallocation(old_size - new_size);
            }
        }
        replacement
    }
}

pub(crate) struct MeasurementStart {
    allocation_count: u64,
    allocated_bytes: u64,
    deallocated_bytes: u64,
    live_bytes: u64,
}

#[derive(Clone, Copy)]
pub(crate) struct AllocationMetrics {
    pub(crate) allocation_count: u64,
    pub(crate) allocated_bytes: u64,
    pub(crate) deallocated_bytes: u64,
    pub(crate) peak_live_bytes: u64,
}

pub(crate) fn begin_measurement() -> MeasurementStart {
    let live_bytes = LIVE_BYTES.load(Ordering::Relaxed);
    PEAK_LIVE_BYTES.store(live_bytes, Ordering::Relaxed);
    MeasurementStart {
        allocation_count: ALLOCATION_COUNT.load(Ordering::Relaxed),
        allocated_bytes: ALLOCATED_BYTES.load(Ordering::Relaxed),
        deallocated_bytes: DEALLOCATED_BYTES.load(Ordering::Relaxed),
        live_bytes,
    }
}

pub(crate) fn finish_measurement(start: MeasurementStart) -> AllocationMetrics {
    AllocationMetrics {
        allocation_count: ALLOCATION_COUNT.load(Ordering::Relaxed) - start.allocation_count,
        allocated_bytes: ALLOCATED_BYTES.load(Ordering::Relaxed) - start.allocated_bytes,
        deallocated_bytes: DEALLOCATED_BYTES.load(Ordering::Relaxed) - start.deallocated_bytes,
        peak_live_bytes: PEAK_LIVE_BYTES
            .load(Ordering::Relaxed)
            .saturating_sub(start.live_bytes),
    }
}

fn record_allocation(bytes: u64) {
    ALLOCATION_COUNT.fetch_add(1, Ordering::Relaxed);
    ALLOCATED_BYTES.fetch_add(bytes, Ordering::Relaxed);
    let live_bytes = LIVE_BYTES.fetch_add(bytes, Ordering::Relaxed) + bytes;
    let mut observed_peak = PEAK_LIVE_BYTES.load(Ordering::Relaxed);
    while live_bytes > observed_peak {
        match PEAK_LIVE_BYTES.compare_exchange_weak(
            observed_peak,
            live_bytes,
            Ordering::Relaxed,
            Ordering::Relaxed,
        ) {
            Ok(_) => break,
            Err(next) => observed_peak = next,
        }
    }
}

fn record_deallocation(bytes: u64) {
    DEALLOCATED_BYTES.fetch_add(bytes, Ordering::Relaxed);
    LIVE_BYTES.fetch_sub(bytes, Ordering::Relaxed);
}
