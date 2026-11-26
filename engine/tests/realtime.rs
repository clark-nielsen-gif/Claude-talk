//! Proves the audio-thread functions never allocate or free memory after
//! set-up, including while routes are added, changed and removed.

use ct_engine::mixer::{connect, disconnect, GainParams, OutputMixer, SourceFanout};
use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;
use std::sync::atomic::Ordering;

struct Counting;
thread_local! {
    static WATCH: Cell<bool> = const { Cell::new(false) };
    static COUNT: Cell<u64> = const { Cell::new(0) };
}
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, l: Layout) -> *mut u8 {
        if WATCH.with(|w| w.get()) {
            COUNT.with(|c| c.set(c.get() + 1));
        }
        unsafe { System.alloc(l) }
    }
    unsafe fn dealloc(&self, p: *mut u8, l: Layout) {
        if WATCH.with(|w| w.get()) {
            COUNT.with(|c| c.set(c.get() + 1));
        }
        unsafe { System.dealloc(p, l) }
    }
    unsafe fn realloc(&self, p: *mut u8, l: Layout, n: usize) -> *mut u8 {
        if WATCH.with(|w| w.get()) {
            COUNT.with(|c| c.set(c.get() + 1));
        }
        unsafe { System.realloc(p, l, n) }
    }
}
#[global_allocator]
static A: Counting = Counting;

fn watched<R>(f: impl FnOnce() -> R) -> (R, u64) {
    COUNT.with(|c| c.set(0));
    WATCH.with(|w| w.set(true));
    let r = f();
    WATCH.with(|w| w.set(false));
    (r, COUNT.with(|c| c.get()))
}

#[test]
fn audio_thread_paths_do_not_allocate() {
    let (mut mic, mut mic_h) = SourceFanout::new(1, 44100);
    let (mut cl, mut cl_h) = SourceFanout::new(2, 48000);
    let (mut out, mut out_h) = OutputMixer::new(101, 2, 480);
    let route = GainParams::new(0.0, false);
    connect(&mut mic_h, &mut out_h, route.clone(), 20.0);
    connect(&mut cl_h, &mut out_h, GainParams::new(-3.0, false), 20.0);
    let input_m = vec![0.1f32; 441];
    let input_c = vec![0.1f32; 960];
    let mut o = vec![0f32; 960];
    // warm-up outside the watch (first command application moves boxes)
    for _ in 0..10 {
        mic.push(Some(&input_m), 1, 441);
        cl.push(Some(&input_c), 2, 480);
        out.render(&mut o);
    }
    let ((), n) = watched(|| {
        for i in 0..2000 {
            mic.push(Some(&input_m), 1, 441);
            cl.push(if i % 7 == 0 { None } else { Some(&input_c) }, 2, 480);
            out.render(&mut o);
            if i == 500 {
                route.gain_db.store(-12.0);
                route.muted.store(true, Ordering::Relaxed);
            }
        }
    });
    assert_eq!(n, 0, "audio paths allocated {n} times");

    // route changes: allocation happens on the control thread only
    disconnect(&mut cl_h, &mut out_h);
    let ((), n) = watched(|| {
        for _ in 0..50 {
            mic.push(Some(&input_m), 1, 441);
            cl.push(Some(&input_c), 2, 480);
            out.render(&mut o);
        }
    });
    assert_eq!(
        n, 0,
        "removing a route allocated/freed on the audio thread ({n})"
    );
    out_h.collect_garbage();
    cl_h.collect_garbage();
    connect(&mut cl_h, &mut out_h, GainParams::new(0.0, false), 20.0);
    let ((), n) = watched(|| {
        for _ in 0..50 {
            mic.push(Some(&input_m), 1, 441);
            cl.push(Some(&input_c), 2, 480);
            out.render(&mut o);
        }
    });
    assert_eq!(n, 0, "adding a route allocated on the audio thread ({n})");
}
