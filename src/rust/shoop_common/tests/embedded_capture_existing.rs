#[shoop_wasm_test_support::shoop_test(
    no_wasm = "requires the native embedded Tracy runtime and filesystem",
    no_tracy = "manages the embedded capture lifecycle directly"
)]
fn embedded_capture_rejects_an_existing_output() {
    let temporary_dir = tempfile::tempdir().expect("create capture output directory");
    let output = temporary_dir.path().join("existing.tracy");
    std::fs::write(&output, b"occupied").expect("occupy capture output");
    let output = output.to_str().expect("temporary path is UTF-8");

    let status = unsafe {
        tracy_client_sys::___tracy_embedded_capture_configure(
            output.as_ptr().cast(),
            output.len(),
            256 * 1024,
            256 * 1024 * 1024,
        )
    };
    assert_eq!(
        status,
        tracy_client_sys::TRACY_EMBEDDED_CAPTURE_OUTPUT_EXISTS
    );

    let length =
        unsafe { tracy_client_sys::___tracy_embedded_capture_get_error(std::ptr::null_mut(), 0) };
    let mut bytes = vec![0_u8; length + 1];
    unsafe {
        tracy_client_sys::___tracy_embedded_capture_get_error(
            bytes.as_mut_ptr().cast(),
            bytes.len(),
        );
    }
    assert_eq!(
        String::from_utf8_lossy(&bytes[..length]),
        "capture output already exists"
    );
    assert_eq!(std::fs::read(&output).unwrap(), b"occupied");
}
