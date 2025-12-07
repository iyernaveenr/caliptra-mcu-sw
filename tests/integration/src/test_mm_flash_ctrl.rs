//! Licensed under the Apache-2.0 license

//! This module tests the PLDM Firmware Update

#[cfg(test)]
pub mod test {
    use std::thread;

    use crate::test::{finish_runtime_hw_model, start_runtime_hw_model, TestParams, TEST_LOCK};
    use mcu_hw_model::McuHwModel;
    use random_port::PortPicker;
    use std::time::Duration;

    #[test]
    pub fn test_imaginary_flash_controller() {
        let feature = "test-mm-flash-ctrl";
        let lock = TEST_LOCK.lock().unwrap();
        lock.fetch_add(1, std::sync::atomic::Ordering::Relaxed);

        let feature = feature.replace("_", "-");
        let mut hw = start_runtime_hw_model(TestParams {
            feature: Some(&feature),
            i3c_port: Some(PortPicker::new().pick().unwrap()),
            imaginary_flash_file_path: Some(std::path::PathBuf::from("imaginary_flash_test.bin")),
            ..Default::default()
        });

        hw.start_i3c_controller();

        // let flash_file = hw.get_imaginary_flash_file();

        let test = finish_runtime_hw_model(&mut hw);

        assert_eq!(0, test);

        // force the compiler to keep the lock
        lock.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    }
}
