use mcu_config::flash::FlashPartition;

// Move to FPGA config:
pub const DRIVER_NUM_MM_FLASH_CTRL: usize = 0x8000_0012; // Driver number for mcu mailbox based flash controller

pub const BLOCK_SIZE: usize = 64 * 1024; // Block size for flash partitions

// Move to FPGA config:
pub const STAGING_PARTITION: FlashPartition = FlashPartition {
    name: "staging_par",
    offset: 0x0000_0000,
    size: (BLOCK_SIZE * 0x200),
    driver_num: DRIVER_NUM_MM_FLASH_CTRL as u32,
};

/* Move to fpga config */
#[macro_export]
macro_rules! flash_partition_list_mm_flash_ctrl {
    ($macro:ident) => {{
        $macro!(0, staging_par, STAGING_PARTITION);
    }};
}
