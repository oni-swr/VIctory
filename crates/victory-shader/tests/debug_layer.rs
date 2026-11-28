//! `VICTORY_D3D12_DEBUG` reaches the device `emit::host::Host` creates: a
//! refused pipeline reports the debug layer's reason, also with several
//! hosts (one per census worker) refusing at once. Software adapter, and a
//! shader that is not one (no title data). This file is its own process: the
//! layer has to be on before the first device of the process exists.

#![cfg(all(windows, feature = "host", feature = "diagnostics"))]

use std::sync::Once;

use victory_shader::emit::host::Host;
use windows::Win32::Graphics::Direct3D12::*;

/// A host on WARP with the layer on, or a diagnostic explaining why it could not start.
fn host() -> Option<Host> {
    static LAYER: Once = Once::new();
    // SAFETY: before any thread of this process reads the environment: every
    // test calls this first, and `Once` blocks the others until it is set.
    LAYER.call_once(|| unsafe { std::env::set_var("VICTORY_D3D12_DEBUG", "1") });
    let host = match Host::new(true) {
        Ok(host) => host,
        Err(reason) => {
            eprintln!("Skipping debug-layer test. WARP initialization failed ({reason})");
            return None;
        }
    };
    if windows::core::Interface::cast::<ID3D12InfoQueue>(&host.device).is_err() {
        eprintln!("Skipping debug-layer test because the debug layer is unavailable on the initialized WARP device.");
        return None;
    }
    Some(host)
}

/// A root signature with nothing in it.
fn empty_root(host: &Host) -> ID3D12RootSignature {
    let desc = D3D12_ROOT_SIGNATURE_DESC::default();
    let mut blob = None;
    // SAFETY: the description is empty and the blob outlives the call.
    unsafe {
        D3D12SerializeRootSignature(&desc, D3D_ROOT_SIGNATURE_VERSION_1, &mut blob, None).unwrap();
        let blob = blob.unwrap();
        let bytes = std::slice::from_raw_parts(
            blob.GetBufferPointer().cast::<u8>(),
            blob.GetBufferSize(),
        );
        host.device.CreateRootSignature(0, bytes).unwrap()
    }
}

#[test]
fn a_refused_pipeline_names_the_debug_layers_reason() {
    let Some(host) = host() else {
        return;
    };
    let root = empty_root(&host);
    let error = host
        .compute_pipeline_with(&root, b"not a shader")
        .expect_err("not a shader");
    assert!(error.contains("; debug layer: id "), "{error}");
}

#[test]
fn hosts_sharing_a_device_keep_their_messages() {
    let Some(first) = host() else {
        return;
    };
    let workers: Vec<_> = (0..8)
        .map(|_| {
            std::thread::spawn(|| {
                let host = host().expect("a second host on the same adapter");
                let root = empty_root(&host);
                (0..25)
                    .map(|_| host.compute_pipeline_with(&root, b"not a shader").unwrap_err())
                    .collect::<Vec<_>>()
            })
        })
        .collect();
    drop(first);
    for worker in workers {
        for error in worker.join().unwrap() {
            // Without the lock, a worker's clear empties another's messages.
            assert!(error.contains("; debug layer: id "), "{error}");
        }
    }
}
